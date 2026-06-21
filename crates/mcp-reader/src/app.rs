//! Server-Komposition. Vereint den SSE/JSON-RPC-Transport und die
//! Betriebs-Health-Endpunkte zu einer einzigen HTTP-App und serviert sie an
//! einem echten TcpListener.
//!
//! Bis hierher waren Transport ([`crate::transport`]) und Health
//! ([`crate::health`]) zwei getrennte Router ohne laufendes Binär. Erst die
//! Komposition macht den Dienst deploybar, denn die K8s-Probes brauchen reale
//! Pfade hinter einem lauschenden Socket.
//!
//! Der reine Zusammenbau ([`app`]) ist ohne Netzwerk per oneshot testbar.
//! [`serve`] bindet die App an einen Listener, sodass `/livez`, `/readyz` und
//! `/startupz` tatsächlich über HTTP antworten.

use std::sync::Arc;

use axum::Router;
use tokio::net::TcpListener;

use crate::auth::AuthResolver;
use crate::health::{HealthState, health_router};
use crate::quota::QuotaBackend;
use crate::transport::{McpService, router};

/// Baut die vollständige HTTP-App.
///
/// Der Transport-Router (`/sse`, `/rpc`) wird mit den Health-Endpunkten
/// (`/livez`, `/readyz`, `/startupz`) zu einem Router verschmolzen. Beide tragen
/// ihren Zustand bereits in sich, deshalb genügt ein `merge`.
pub fn app<A, B>(service: Arc<McpService<A, B>>, health: Arc<HealthState>) -> Router
where
    A: AuthResolver + Send + Sync + 'static,
    B: QuotaBackend + Send + Sync + 'static,
{
    router(service).merge(health_router(health))
}

/// Serviert eine fertige App am Listener bis zum Prozessende.
///
/// Dünne Hülle um `axum::serve`. Der Listener ist bewusst von aussen
/// hereingereicht, damit Aufrufer (Tests wie `main`) die Adresse selbst wählen.
pub async fn serve(listener: TcpListener, app: Router) -> std::io::Result<()> {
    axum::serve(listener, app).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{ClaimRecord, Role, StaticAuthResolver};
    use crate::quota::{QuotaError, QuotaPolicy, RateLimiter};
    use crate::registry::Registry;
    use crate::temporal::TemporalResolver;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use fedlex_store::token_bucket::{Acquisition, BucketParams};
    use std::net::SocketAddr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tower::ServiceExt;

    /// Quota-Backend, das im Test stets erlaubt.
    struct AllowingBackend;

    impl QuotaBackend for AllowingBackend {
        async fn try_acquire(
            &self,
            _key: &str,
            _params: BucketParams,
            _cost: u32,
            _now_ms: u64,
        ) -> Result<Acquisition, QuotaError> {
            Ok(Acquisition {
                allowed: true,
                remaining: 99,
                retry_after_ms: 0,
            })
        }
    }

    fn service() -> Arc<McpService<StaticAuthResolver, AllowingBackend>> {
        let registry = Registry::new();
        let auth = StaticAuthResolver::new().with_credential(
            "token-a",
            ClaimRecord {
                tenant: "kanzlei-a".into(),
                session: "sess-1".into(),
                role: Role::Reader,
            },
        );
        let limiter = RateLimiter::with_policy(AllowingBackend, QuotaPolicy::default());
        let temporal = TemporalResolver::new(time::macros::date!(2024 - 01 - 01));
        Arc::new(McpService::new(registry, auth, limiter, temporal))
    }

    #[tokio::test]
    async fn merged_app_serves_health_and_transport() {
        let health = Arc::new(HealthState::new());
        let app = app(service(), Arc::clone(&health));

        let livez = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/livez")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(livez.status(), StatusCode::OK);

        let sse = app
            .oneshot(Request::builder().uri("/sse").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(sse.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn startup_flips_to_ready_only_after_mark_started() {
        let health = Arc::new(HealthState::new());
        let app = app(service(), Arc::clone(&health));

        let before = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/startupz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(before.status(), StatusCode::SERVICE_UNAVAILABLE);

        health.mark_started();

        let after = app
            .oneshot(
                Request::builder()
                    .uri("/startupz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(after.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn served_over_a_real_socket_answers_livez() {
        let health = Arc::new(HealthState::new());
        let app = app(service(), Arc::clone(&health));

        let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            serve(listener, app).await.unwrap();
        });

        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET /livez HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();

        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let text = String::from_utf8_lossy(&response);
        assert!(text.starts_with("HTTP/1.1 200 OK"), "Antwort war: {text}");
        assert!(text.contains("\"status\":\"alive\""), "Antwort war: {text}");
    }
}
