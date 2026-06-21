//! Betriebs-Health-Endpunkte (Day-2, M10).
//!
//! Drei getrennte Signale, wie Kubernetes sie erwartet.
//!
//! `livez` (Liveness) sagt nur, ob der Prozess noch arbeitet. Ein Fehlschlag
//! bedeutet Neustart. Die Prüfung hängt an keiner Abhängigkeit, sonst würde ein
//! kurzer Ausfall von Oxigraph oder Redis fälschlich einen Neustart auslösen.
//!
//! `readyz` (Readiness) sagt, ob der Dienst gerade Verkehr bedienen kann. Hier
//! laufen die Bereitschaftsprüfungen der Abhängigkeiten. Ein Fehlschlag nimmt
//! die Instanz aus dem Load-Balancer, ohne sie zu töten.
//!
//! `startupz` (Startup) schützt langsam startende Container. Bis der Aufwärmlauf
//! fertig ist, antwortet die Prüfung mit 503, damit Liveness nicht zu früh
//! greift.
//!
//! Der Kern ([`HealthState`]) ist ohne Netzwerk testbar. Der axum-Router
//! ([`health_router`]) verdrahtet ihn an HTTP.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use serde_json::json;

/// Eine benannte Bereitschaftsprüfung einer Abhängigkeit.
///
/// Die Implementierung kapselt das Netzwerk (etwa ein Redis-Ping oder eine
/// triviale SPARQL-Abfrage). Der Health-Kern kennt nur den Namen und das
/// boolesche Ergebnis und bleibt so frei von Backend-Typen.
#[async_trait::async_trait]
pub trait ReadinessProbe: Send + Sync {
    /// Name der geprüften Abhängigkeit, etwa `oxigraph` oder `redis`.
    fn name(&self) -> &str;
    /// `true`, wenn die Abhängigkeit gerade erreichbar und nutzbar ist.
    async fn ready(&self) -> bool;
}

/// Gemeinsamer Health-Zustand hinter den drei Endpunkten.
///
/// `live` und `started` sind atomare Schalter. `probes` hält die
/// Bereitschaftsprüfungen, die `readyz` der Reihe nach abfragt.
pub struct HealthState {
    live: AtomicBool,
    started: AtomicBool,
    probes: Vec<Arc<dyn ReadinessProbe>>,
}

impl HealthState {
    /// Neuer Zustand. Der Prozess gilt als lebendig, der Aufwärmlauf als noch
    /// nicht fertig.
    pub fn new() -> Self {
        Self {
            live: AtomicBool::new(true),
            started: AtomicBool::new(false),
            probes: Vec::new(),
        }
    }

    /// Fügt eine Bereitschaftsprüfung hinzu (Builder-Stil).
    pub fn with_probe(mut self, probe: Arc<dyn ReadinessProbe>) -> Self {
        self.probes.push(probe);
        self
    }

    /// Meldet den Aufwärmlauf als abgeschlossen. Ab jetzt antwortet `startupz`
    /// mit 200.
    pub fn mark_started(&self) {
        self.started.store(true, Ordering::SeqCst);
    }

    /// Meldet den Prozess als nicht mehr lebensfähig. Liveness schlägt fehl und
    /// die Orchestrierung startet die Instanz neu.
    pub fn mark_unhealthy(&self) {
        self.live.store(false, Ordering::SeqCst);
    }

    /// Liveness-Zustand.
    pub fn is_live(&self) -> bool {
        self.live.load(Ordering::SeqCst)
    }

    /// Startup-Zustand.
    pub fn is_started(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }

    /// Fragt alle Bereitschaftsprüfungen ab und sammelt die Namen der
    /// fehlschlagenden. Eine leere Liste bedeutet bereit.
    pub async fn failing_probes(&self) -> Vec<String> {
        let mut failing = Vec::new();
        for probe in &self.probes {
            if !probe.ready().await {
                failing.push(probe.name().to_string());
            }
        }
        failing
    }
}

impl Default for HealthState {
    fn default() -> Self {
        Self::new()
    }
}

/// GET `/livez`. 200, solange der Prozess lebt, sonst 503.
async fn livez_handler(
    State(state): State<Arc<HealthState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    if state.is_live() {
        (StatusCode::OK, Json(json!({ "status": "alive" })))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "dead" })),
        )
    }
}

/// GET `/startupz`. 200, sobald der Aufwärmlauf fertig ist, sonst 503.
async fn startupz_handler(
    State(state): State<Arc<HealthState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    if state.is_started() {
        (StatusCode::OK, Json(json!({ "status": "started" })))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "starting" })),
        )
    }
}

/// GET `/readyz`. 200, wenn alle Abhängigkeiten bereit sind, sonst 503 mit der
/// Liste der fehlschlagenden Prüfungen.
async fn readyz_handler(
    State(state): State<Arc<HealthState>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let failing = state.failing_probes().await;
    if failing.is_empty() {
        (StatusCode::OK, Json(json!({ "status": "ready" })))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "not_ready", "failing": failing })),
        )
    }
}

/// Baut den Health-Router über einem geteilten [`HealthState`].
pub fn health_router(state: Arc<HealthState>) -> Router {
    Router::new()
        .route("/livez", get(livez_handler))
        .route("/readyz", get(readyz_handler))
        .route("/startupz", get(startupz_handler))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    /// Prüfung mit fest verdrahtetem Ergebnis.
    struct StaticProbe {
        name: &'static str,
        ready: bool,
    }

    #[async_trait::async_trait]
    impl ReadinessProbe for StaticProbe {
        fn name(&self) -> &str {
            self.name
        }
        async fn ready(&self) -> bool {
            self.ready
        }
    }

    async fn get(router: Router, uri: &str) -> (StatusCode, serde_json::Value) {
        let resp = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        (status, v)
    }

    #[tokio::test]
    async fn liveness_is_ok_until_marked_unhealthy() {
        let state = Arc::new(HealthState::new());
        let (status, body) = get(health_router(state.clone()), "/livez").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "alive");

        state.mark_unhealthy();
        let (status, body) = get(health_router(state), "/livez").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["status"], "dead");
    }

    #[tokio::test]
    async fn startup_is_unavailable_until_marked_started() {
        let state = Arc::new(HealthState::new());
        let (status, body) = get(health_router(state.clone()), "/startupz").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["status"], "starting");

        state.mark_started();
        let (status, body) = get(health_router(state), "/startupz").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "started");
    }

    #[tokio::test]
    async fn readiness_without_probes_is_ok() {
        let state = Arc::new(HealthState::new());
        let (status, body) = get(health_router(state), "/readyz").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ready");
    }

    #[tokio::test]
    async fn readiness_is_ok_when_all_probes_ready() {
        let state = Arc::new(
            HealthState::new()
                .with_probe(Arc::new(StaticProbe {
                    name: "oxigraph",
                    ready: true,
                }))
                .with_probe(Arc::new(StaticProbe {
                    name: "redis",
                    ready: true,
                })),
        );
        let (status, _) = get(health_router(state), "/readyz").await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn readiness_fails_and_names_the_broken_dependency() {
        let state = Arc::new(
            HealthState::new()
                .with_probe(Arc::new(StaticProbe {
                    name: "oxigraph",
                    ready: true,
                }))
                .with_probe(Arc::new(StaticProbe {
                    name: "redis",
                    ready: false,
                })),
        );
        let (status, body) = get(health_router(state), "/readyz").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["status"], "not_ready");
        assert_eq!(body["failing"][0], "redis");
    }
}
