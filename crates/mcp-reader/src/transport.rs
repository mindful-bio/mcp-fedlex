//! SSE/JSON-RPC-Transport (`sseHandler`).
//!
//! Die Eingangstür des Readers. Ein MCP-Client öffnet einen SSE-Strom und
//! erhält die POST-Adresse für JSON-RPC-Nachrichten. Über diese Tür laufen drei
//! Methoden. `initialize` (Handshake), `tools/list` (RBAC-gefiltert) und
//! `tools/call` (Quota-gedrosselt, durch das Provenance-Gate).
//!
//! Die Kette pro Aufruf. Credential verifizieren (ADR-002, Identität nie aus
//! LLM-Parametern), Quota prüfen (verteiltes Token-Bucket, fail-closed),
//! Anfrage temporal stempeln (Stichtag), dann Dispatch durch die [`Registry`].
//! Jeder Fehler wird zu einer lenkenden Antwort statt eines Transport-Crashs.
//!
//! Der reine Kern ([`McpService`]) ist ohne Netzwerk testbar. Der axum-Router
//! ([`router`]) verdrahtet ihn an HTTP und SSE.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header::AUTHORIZATION, HeaderMap};
use axum::response::sse::{Event, Sse};
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use time::macros::format_description;
use time::Date;

use crate::auth::AuthResolver;
use crate::quota::QuotaBackend;
use crate::quota::RateLimiter;
use crate::registry::Registry;
use crate::temporal::TemporalResolver;
use crate::tool::ToolContext;

/// JSON-RPC-Fehlercodes. Die negativen Standardcodes plus ein eigener für Auth.
mod codes {
    /// Nachricht war kein gültiges JSON.
    pub const PARSE_ERROR: i32 = -32700;
    /// Methode unbekannt.
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Parameter ungültig.
    pub const INVALID_PARAMS: i32 = -32602;
    /// Credential fehlt oder ist ungültig (anwendungsspezifisch).
    pub const UNAUTHORIZED: i32 = -32001;
}

/// Eine eingehende JSON-RPC-2.0-Anfrage.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    /// Korrelations-Id der Anfrage (Zahl, String oder null).
    #[serde(default)]
    pub id: Value,
    /// Aufgerufene Methode.
    pub method: String,
    /// Parameter der Methode.
    #[serde(default)]
    pub params: Value,
}

/// Eine JSON-RPC-2.0-Fehlerhülle.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct JsonRpcError {
    /// Fehlercode.
    pub code: i32,
    /// Menschenlesbare Fehlermeldung.
    pub message: String,
}

/// Eine ausgehende JSON-RPC-2.0-Antwort. Entweder `result` oder `error`.
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    /// Protokoll-Marker, immer "2.0".
    pub jsonrpc: &'static str,
    /// Korrelations-Id der zugehörigen Anfrage.
    pub id: Value,
    /// Ergebnis bei Erfolg.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Fehler bei Misserfolg.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    fn ok(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

/// Der zustandslose MCP-Dienst. Bündelt Registry, Auth, Quota und Temporal.
pub struct McpService<A: AuthResolver, B: QuotaBackend> {
    registry: Registry,
    auth: A,
    limiter: RateLimiter<B>,
    temporal: TemporalResolver,
}

impl<A: AuthResolver, B: QuotaBackend> McpService<A, B> {
    /// Erzeugt den Dienst aus seinen Bausteinen.
    pub fn new(
        registry: Registry,
        auth: A,
        limiter: RateLimiter<B>,
        temporal: TemporalResolver,
    ) -> Self {
        Self {
            registry,
            auth,
            limiter,
            temporal,
        }
    }

    /// Verarbeitet eine JSON-RPC-Anfrage Ende zu Ende.
    ///
    /// `credential` stammt aus dem Transport (Authorization-Header), nie aus den
    /// Parametern. `now_ms` ist die Wanduhr für das Token-Bucket.
    pub async fn handle(
        &self,
        credential: Option<&str>,
        req: JsonRpcRequest,
        now_ms: u64,
    ) -> JsonRpcResponse {
        // Identität zuerst. Ohne gültiges Credential keine Methode.
        let Some(cred) = credential else {
            return JsonRpcResponse::err(req.id, codes::UNAUTHORIZED, "missing credential");
        };
        let claims = match self.auth.verify(cred) {
            Ok(c) => c,
            Err(e) => return JsonRpcResponse::err(req.id, codes::UNAUTHORIZED, e.to_string()),
        };

        match req.method.as_str() {
            "initialize" => JsonRpcResponse::ok(
                req.id,
                json!({
                    "protocolVersion": "2024-11-05",
                    "serverInfo": { "name": "mcp-fedlex-reader", "version": env!("CARGO_PKG_VERSION") },
                    "capabilities": { "tools": {} }
                }),
            ),

            "tools/list" => JsonRpcResponse::ok(
                req.id,
                json!({ "tools": self.registry.list_tools(claims.role()) }),
            ),

            "tools/call" => {
                // Quota vor jeder Arbeit. Schlüssel nur aus dem Claim (ADR-002).
                let decision = self.limiter.check(&claims, now_ms).await;
                if !decision.allowed {
                    // Lenkende Antwort statt Transport-Fehler, damit das LLM
                    // sich selbst drosseln kann.
                    return JsonRpcResponse::ok(
                        req.id,
                        json!({
                            "error": "rate limit exceeded",
                            "hint": "Quota erschoepft, bitte vor dem naechsten Aufruf warten.",
                            "retry_after_ms": decision.retry_after_ms,
                            "degraded": decision.degraded,
                        }),
                    );
                }

                let Some(name) = req.params.get("name").and_then(Value::as_str) else {
                    return JsonRpcResponse::err(
                        req.id,
                        codes::INVALID_PARAMS,
                        "missing tool name",
                    );
                };
                let args = req
                    .params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));

                // Optionaler Stichtag. Ungültiges Datum ist ein Parameterfehler.
                let requested_as_of = match req.params.get("as_of").and_then(Value::as_str) {
                    Some(s) => match Date::parse(s, format_description!("[year]-[month]-[day]")) {
                        Ok(d) => Some(d),
                        Err(_) => {
                            return JsonRpcResponse::err(
                                req.id,
                                codes::INVALID_PARAMS,
                                "as_of must be an ISO date (YYYY-MM-DD)",
                            )
                        }
                    },
                    None => None,
                };

                let stamp = self.temporal.stamp(requested_as_of);
                let ctx = ToolContext { claims, stamp };
                // Dispatch liefert immer valides JSON (Erfolg mit Provenance oder
                // graceful { error, hint }). Es wird unverändert als result gereicht.
                let result = self.registry.dispatch(&ctx, name, args).await;
                JsonRpcResponse::ok(req.id, result)
            }

            other => JsonRpcResponse::err(
                req.id,
                codes::METHOD_NOT_FOUND,
                format!("unknown method `{other}`"),
            ),
        }
    }
}

/// Liest das Bearer-Token aus dem Authorization-Header.
fn bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(str::to_owned)
}

/// Aktuelle Wanduhr in Millisekunden seit Epoch.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// POST `/rpc`. Verarbeitet eine JSON-RPC-Nachricht.
async fn rpc_handler<A, B>(
    State(svc): State<Arc<McpService<A, B>>>,
    headers: HeaderMap,
    body: Bytes,
) -> Json<JsonRpcResponse>
where
    A: AuthResolver + Send + Sync + 'static,
    B: QuotaBackend + Send + Sync + 'static,
{
    let req: JsonRpcRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return Json(JsonRpcResponse::err(
                Value::Null,
                codes::PARSE_ERROR,
                e.to_string(),
            ))
        }
    };
    let cred = bearer(&headers);
    Json(svc.handle(cred.as_deref(), req, now_ms()).await)
}

/// GET `/sse`. Eröffnet den Ereignis-Strom und nennt die POST-Adresse.
///
/// Konvention des MCP-SSE-Transports. Der Server sendet zuerst ein
/// `endpoint`-Ereignis mit der URL, an die der Client seine JSON-RPC-Nachrichten
/// POSTet.
async fn sse_handler() -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let endpoint = Event::default().event("endpoint").data("/rpc");
    let stream = stream::iter(vec![Ok(endpoint)]);
    Sse::new(stream)
}

/// Baut den axum-Router über einem geteilten [`McpService`].
pub fn router<A, B>(service: Arc<McpService<A, B>>) -> Router
where
    A: AuthResolver + Send + Sync + 'static,
    B: QuotaBackend + Send + Sync + 'static,
{
    Router::new()
        .route("/sse", get(sse_handler))
        .route("/rpc", post(rpc_handler::<A, B>))
        .with_state(service)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{ClaimRecord, Role, StaticAuthResolver};
    use crate::quota::{QuotaError, QuotaPolicy};
    use crate::registry::Registry;
    use crate::tool::{McpTool, ToolError, ToolPool};
    use async_trait::async_trait;
    use fedlex_core::{Eli, Response};
    use fedlex_store::token_bucket::{Acquisition, BucketParams};
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Quota-Backend, das wahlweise immer erlaubt oder immer ablehnt.
    struct MockBackend {
        deny: AtomicBool,
    }

    impl MockBackend {
        fn allowing() -> Self {
            Self {
                deny: AtomicBool::new(false),
            }
        }
        fn denying() -> Self {
            Self {
                deny: AtomicBool::new(true),
            }
        }
    }

    impl QuotaBackend for MockBackend {
        async fn try_acquire(
            &self,
            _key: &str,
            _params: BucketParams,
            _cost: u32,
            _now_ms: u64,
        ) -> Result<Acquisition, QuotaError> {
            if self.deny.load(Ordering::SeqCst) {
                Ok(Acquisition {
                    allowed: false,
                    remaining: 0,
                    retry_after_ms: 1000,
                })
            } else {
                Ok(Acquisition {
                    allowed: true,
                    remaining: 99,
                    retry_after_ms: 0,
                })
            }
        }
    }

    /// Ein Tool, das eine Antwort mit Provenance liefert.
    struct ReadArticle;

    #[async_trait]
    impl McpTool for ReadArticle {
        fn name(&self) -> &str {
            "read_article"
        }
        fn pool(&self) -> ToolPool {
            ToolPool::LocalNavigation
        }
        fn schema(&self) -> Value {
            json!({ "type": "object" })
        }
        async fn execute(
            &self,
            ctx: &ToolContext,
            _args: Value,
        ) -> Result<Response<Value>, ToolError> {
            let eli = Eli::new("eli/cc/1999/404")
                .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;
            let prov = ctx.stamp.into_provenance(eli);
            Ok(Response::new(json!({ "text": "Art. 1 BV" }), prov))
        }
    }

    fn auth() -> StaticAuthResolver {
        StaticAuthResolver::new().with_credential(
            "token-a",
            ClaimRecord {
                tenant: "kanzlei-a".into(),
                session: "sess-1".into(),
                role: Role::Reader,
            },
        )
    }

    fn service(backend: MockBackend) -> McpService<StaticAuthResolver, MockBackend> {
        let mut registry = Registry::new();
        registry.register(Arc::new(ReadArticle));
        let limiter = RateLimiter::with_policy(backend, QuotaPolicy::default());
        let temporal = TemporalResolver::new(time::macros::date!(2024 - 01 - 01));
        McpService::new(registry, auth(), limiter, temporal)
    }

    fn req(id: i64, method: &str, params: Value) -> JsonRpcRequest {
        JsonRpcRequest {
            id: json!(id),
            method: method.into(),
            params,
        }
    }

    #[tokio::test]
    async fn missing_credential_is_unauthorized() {
        let svc = service(MockBackend::allowing());
        let resp = svc.handle(None, req(1, "tools/list", json!({})), 0).await;
        assert_eq!(resp.error.unwrap().code, codes::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn forged_credential_is_unauthorized() {
        let svc = service(MockBackend::allowing());
        let resp = svc
            .handle(Some("forged"), req(1, "tools/list", json!({})), 0)
            .await;
        assert_eq!(resp.error.unwrap().code, codes::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn initialize_returns_server_info() {
        let svc = service(MockBackend::allowing());
        let resp = svc
            .handle(Some("token-a"), req(1, "initialize", json!({})), 0)
            .await;
        let result = resp.result.unwrap();
        assert_eq!(result["serverInfo"]["name"], "mcp-fedlex-reader");
        assert!(result["capabilities"]["tools"].is_object());
    }

    #[tokio::test]
    async fn tools_list_is_role_filtered() {
        let svc = service(MockBackend::allowing());
        let resp = svc
            .handle(Some("token-a"), req(1, "tools/list", json!({})), 0)
            .await;
        let tools = resp.result.unwrap()["tools"].clone();
        assert_eq!(tools.as_array().unwrap().len(), 1);
        assert_eq!(tools[0]["name"], "read_article");
    }

    #[tokio::test]
    async fn tools_call_carries_provenance_end_to_end() {
        let svc = service(MockBackend::allowing());
        let resp = svc
            .handle(
                Some("token-a"),
                req(
                    7,
                    "tools/call",
                    json!({ "name": "read_article", "as_of": "2020-01-01" }),
                ),
                0,
            )
            .await;
        let result = resp.result.unwrap();
        // Das Provenance-Gate trägt strukturell bis ins Wire-Format.
        assert_eq!(result["data"]["text"], "Art. 1 BV");
        assert_eq!(result["provenance"]["eli"], "eli/cc/1999/404");
        assert_eq!(resp.id, json!(7));
    }

    #[tokio::test]
    async fn quota_denied_yields_steering_result_not_crash() {
        let svc = service(MockBackend::denying());
        let resp = svc
            .handle(
                Some("token-a"),
                req(1, "tools/call", json!({ "name": "read_article" })),
                0,
            )
            .await;
        // Kein Transport-Fehler. Eine lenkende Antwort mit Wartehinweis.
        let result = resp.result.unwrap();
        assert_eq!(result["error"], "rate limit exceeded");
        assert!(result["retry_after_ms"].as_i64().unwrap() > 0);
    }

    #[tokio::test]
    async fn invalid_as_of_is_param_error() {
        let svc = service(MockBackend::allowing());
        let resp = svc
            .handle(
                Some("token-a"),
                req(
                    1,
                    "tools/call",
                    json!({ "name": "read_article", "as_of": "gestern" }),
                ),
                0,
            )
            .await;
        assert_eq!(resp.error.unwrap().code, codes::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn unknown_method_is_method_not_found() {
        let svc = service(MockBackend::allowing());
        let resp = svc
            .handle(Some("token-a"), req(1, "telepathy/read", json!({})), 0)
            .await;
        assert_eq!(resp.error.unwrap().code, codes::METHOD_NOT_FOUND);
    }

    // HTTP-Ebene. Beweist die Verdrahtung von Header-Extraktion und Routing.
    #[tokio::test]
    async fn http_rpc_endpoint_enforces_auth_and_dispatches() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        let svc = Arc::new(service(MockBackend::allowing()));
        let app = router(svc);

        // Ohne Authorization-Header. Unauthorized auf Anwendungsebene.
        let anon = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/rpc")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "jsonrpc": "2.0", "id": 1, "method": "tools/list"
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(anon.status(), StatusCode::OK);
        let body = axum::body::to_bytes(anon.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["error"]["code"], codes::UNAUTHORIZED);

        // Mit gültigem Bearer-Token. Dispatch liefert die rollengefilterte Liste.
        let authed = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/rpc")
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer token-a")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "jsonrpc": "2.0", "id": 2, "method": "tools/list"
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = axum::body::to_bytes(authed.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["result"]["tools"][0]["name"], "read_article");
        assert_eq!(v["id"], json!(2));
    }
}
