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
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header::AUTHORIZATION, HeaderMap};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;

use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use time::macros::format_description;
use time::Date;

use fedlex_telemetry::{AttributeAllowlist, SpanScrubber};

use crate::auth::{AuthResolver, VerifiedClaims};
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

/// Scrubber für das Call-Log. Baseline-Allowlist plus die pseudonymen
/// Audit-Identitäten (Mandant, Session) aus dem geprüften Claim. Roh-Tool-
/// Argumente und Response-Inhalte stehen nie auf der Allowlist (ADR-001).
fn call_log_scrubber() -> SpanScrubber {
    SpanScrubber::new(
        AttributeAllowlist::baseline()
            .allow("auth.tenant")
            .allow("auth.session"),
    )
}

/// Baut die Audit-Logzeile eines `tools/call` als einzeiliges JSON.
///
/// ELI und Stichtag stammen aus der Provenance der Antwort (der verifizierten
/// Quelle), nie aus den rohen Tool-Argumenten. Jede Zeile passiert den
/// PII-Scrubber, damit künftige Attribute fail-closed redacted bleiben.
fn call_log_line(claims: &VerifiedClaims, tool: &str, result: &Value, duration_ms: u64) -> String {
    let prov = |key: &str| -> String {
        result
            .get("provenance")
            .and_then(|p| p.get(key))
            .and_then(Value::as_str)
            .unwrap_or("-")
            .to_string()
    };
    let outcome = if result.get("error").is_some() {
        "error"
    } else {
        "ok"
    };
    let span = call_log_scrubber().scrub(
        "tools/call",
        [
            ("tool.name".to_string(), tool.to_string()),
            ("auth.role".to_string(), format!("{:?}", claims.role())),
            (
                "auth.tenant".to_string(),
                claims.tenant().as_str().to_string(),
            ),
            (
                "auth.session".to_string(),
                claims.session().as_str().to_string(),
            ),
            ("provenance.eli".to_string(), prov("eli")),
            ("provenance.valid_as_of".to_string(), prov("valid_as_of")),
            ("outcome".to_string(), outcome.to_string()),
            ("span.duration_ms".to_string(), duration_ms.to_string()),
        ],
    );
    let mut obj = serde_json::Map::new();
    obj.insert("event".into(), Value::String(span.name().to_string()));
    for (key, value) in span.attributes() {
        obj.insert(key.clone(), Value::String(value.clone()));
    }
    Value::Object(obj).to_string()
}

/// Der zustandslose MCP-Dienst. Bündelt Registry, Auth, Quota und Temporal.
pub struct McpService<A: AuthResolver, B: QuotaBackend> {
    registry: Registry,
    auth: A,
    limiter: RateLimiter<B>,
    temporal: TemporalResolver,
    /// Ausgehandelte Default-Protokollversion, wenn der Client keine nennt
    /// (Migrations-Runbook Phase 2). Aus der Umgebung (`MCP_PROTOCOL_DEFAULT`)
    /// aufgelöst, fällt fail-safe auf [`crate::protocol::DEFAULT_PROTOCOL_VERSION`].
    protocol_default: &'static str,
}

impl<A: AuthResolver, B: QuotaBackend> McpService<A, B> {
    /// Erzeugt den Dienst aus seinen Bausteinen. Die Default-Protokollversion
    /// wird aus der Umgebung aufgelöst (Config-Flip statt Code-Redeploy).
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
            protocol_default: crate::protocol::default_protocol_version(),
        }
    }

    /// Wie [`Self::new`], aber mit explizit gesetzter Default-Protokollversion
    /// (deterministisch für Tests, ohne Umgebungsvariablen).
    pub fn with_protocol_default(
        registry: Registry,
        auth: A,
        limiter: RateLimiter<B>,
        temporal: TemporalResolver,
        protocol_default: &'static str,
    ) -> Self {
        Self {
            registry,
            auth,
            limiter,
            temporal,
            protocol_default,
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
            "initialize" => {
                // Versions-Negotiation (Migrations-Runbook Phase 2). Der Client
                // darf `protocolVersion` nennen; tut er es nicht (heutiger ansV-
                // Fall), gilt die konfigurierte Default-Version. Eine unbekannte/
                // zu neue Version beantworten wir spec-konform mit unserer
                // höchsten — kein harter Fehler. Capabilities bleiben ehrlich
                // (nur `tools`, solange nichts anderes implementiert ist).
                let requested = req
                    .params
                    .get("protocolVersion")
                    .and_then(Value::as_str);
                let negotiated = crate::protocol::negotiate(requested, self.protocol_default);
                JsonRpcResponse::ok(
                    req.id,
                    json!({
                        "protocolVersion": negotiated,
                        "serverInfo": { "name": "mcp-fedlex-reader", "version": env!("CARGO_PKG_VERSION") },
                        "capabilities": { "tools": {} }
                    }),
                )
            }

            "tools/list" => JsonRpcResponse::ok(
                req.id,
                json!({ "tools": self.registry.list_tools(claims.role()) }),
            ),

            "tools/call" => {
                // Tool-Namen ZUERST bestimmen, damit das Quota-Gewicht
                // pool-abhängig gebucht werden kann (ADR-006). Der Name kommt aus
                // den Parametern, das Gewicht jedoch NICHT — es wird serverseitig
                // aus dem registrierten Pool abgeleitet (claim-/pool-gebunden,
                // von keinem LLM-Parameter senkbar, ADR-002).
                let Some(name) = req.params.get("name").and_then(Value::as_str) else {
                    return JsonRpcResponse::err(
                        req.id,
                        codes::INVALID_PARAMS,
                        "missing tool name",
                    );
                };

                // Live-Discovery wiegt schwerer als lokale Navigation. Unbekannte
                // Tools wiegen 1 (der Dispatch lehnt sie ohnehin graceful ab).
                let cost = self
                    .registry
                    .pool_of(name)
                    .map(|pool| pool.cost_weight())
                    .unwrap_or(1);

                // Quota vor jeder Arbeit. Schlüssel nur aus dem Claim (ADR-002),
                // Gewicht aus dem Pool (ADR-006).
                let decision = self.limiter.check_weighted(&claims, cost, now_ms).await;
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
                let started = Instant::now();
                let result = self.registry.dispatch(&ctx, name, args).await;
                // Audit-Logzeile pro Call. Damit ist server-seitig belegbar,
                // welcher Mandant wann welche Norm in welcher Fassung gefetcht hat.
                println!(
                    "{}",
                    call_log_line(
                        &ctx.claims,
                        name,
                        &result,
                        started.elapsed().as_millis() as u64,
                    )
                );
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
///
/// **Rückgabetyp [`Response`] statt `Json<…>` (Migrations-Runbook Phase 3.2).**
/// Heute antwortet jeder Pfad weiterhin **HTTP 200 + JSON-Body** — bit-identisch
/// zum bisherigen Verhalten und von Alt-Clients (ansV, syllogismus-fedlex) nicht
/// unterscheidbar. Der allgemeinere Typ ist die strukturelle Vorbedingung, um in
/// den späteren Phasen ohne erneuten Signatur-Umbau die von der Ziel-Revision
/// geforderten Status-Codes auszudrücken: **202/204** ohne Body (Notifications,
/// 5.1), **400** (unbekannte Protokollversion), **403** (ungültiger `Origin`),
/// **401**. Bis dahin bleibt es bei 200+JSON.
async fn rpc_handler<A, B>(
    State(svc): State<Arc<McpService<A, B>>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response
where
    A: AuthResolver + Send + Sync + 'static,
    B: QuotaBackend + Send + Sync + 'static,
{
    let req: JsonRpcRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            // Parse-Fehler bleibt wie bisher 200 + JSON-RPC-Error-Body.
            return Json(JsonRpcResponse::err(
                Value::Null,
                codes::PARSE_ERROR,
                e.to_string(),
            ))
            .into_response();
        }
    };
    let cred = bearer(&headers);
    Json(svc.handle(cred.as_deref(), req, now_ms()).await).into_response()
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

    /// Quota-Backend, das das zuletzt angeforderte Cost-Gewicht festhält. Damit
    /// lässt sich belegen, dass der Transport pool-abhängig gewichtet abbucht
    /// (ADR-006), ohne ein echtes Token-Bucket zu brauchen.
    struct CostSpyBackend {
        last_cost: std::sync::atomic::AtomicU32,
    }

    impl CostSpyBackend {
        fn new() -> Self {
            Self {
                last_cost: std::sync::atomic::AtomicU32::new(0),
            }
        }
    }

    impl QuotaBackend for CostSpyBackend {
        async fn try_acquire(
            &self,
            _key: &str,
            _params: BucketParams,
            cost: u32,
            _now_ms: u64,
        ) -> Result<Acquisition, QuotaError> {
            self.last_cost.store(cost, Ordering::SeqCst);
            Ok(Acquisition {
                allowed: true,
                remaining: 99,
                retry_after_ms: 0,
            })
        }
    }

    /// Ein Discovery-Tool (Pool::Discovery) für den Quota-Gewichtstest.
    struct FakeDiscovery;

    #[async_trait]
    impl McpTool for FakeDiscovery {
        fn name(&self) -> &str {
            "search_law"
        }
        fn pool(&self) -> ToolPool {
            ToolPool::Discovery
        }
        fn schema(&self) -> Value {
            json!({ "type": "object" })
        }
        async fn execute(
            &self,
            ctx: &ToolContext,
            _args: Value,
        ) -> Result<Response<Value>, ToolError> {
            let eli = Eli::new("eli/cc").map_err(|e| ToolError::InvalidArguments(e.to_string()))?;
            // Discovery liefert Hinweis-Provenance, kein Beleg (ADR-006).
            let prov = ctx.stamp.into_hint_provenance(eli);
            Ok(Response::new(json!({ "hits": [] }), prov))
        }
    }

    /// Ein JOLux-Metadaten-Tool (Pool::JoluxMetadata) für den Quota-Gewichtstest.
    struct FakeMetadata;

    #[async_trait]
    impl McpTool for FakeMetadata {
        fn name(&self) -> &str {
            "check_in_force"
        }
        fn pool(&self) -> ToolPool {
            ToolPool::JoluxMetadata
        }
        fn schema(&self) -> Value {
            json!({ "type": "object" })
        }
        async fn execute(
            &self,
            ctx: &ToolContext,
            _args: Value,
        ) -> Result<Response<Value>, ToolError> {
            let eli = Eli::new("eli/cc/2017/762")
                .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;
            // Metadaten-Tools belegen einen bekannten Erlass: Norm-Provenance (ADR-007).
            let prov = ctx.stamp.into_provenance(eli);
            Ok(Response::new(json!({ "in_force": true }), prov))
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

    /// Baut einen Dienst mit explizit gesetzter Default-Protokollversion, um die
    /// Negotiation deterministisch (ohne Umgebungsvariablen) zu prüfen.
    fn service_with_default(
        backend: MockBackend,
        default: &'static str,
    ) -> McpService<StaticAuthResolver, MockBackend> {
        let mut registry = Registry::new();
        registry.register(Arc::new(ReadArticle));
        let limiter = RateLimiter::with_policy(backend, QuotaPolicy::default());
        let temporal = TemporalResolver::new(time::macros::date!(2024 - 01 - 01));
        McpService::with_protocol_default(registry, auth(), limiter, temporal, default)
    }

    #[tokio::test]
    async fn initialize_without_client_version_uses_default() {
        // Heutiger ansV-Fall: kein protocolVersion im initialize → Default.
        let svc = service(MockBackend::allowing());
        let resp = svc
            .handle(Some("token-a"), req(1, "initialize", json!({})), 0)
            .await;
        assert_eq!(
            resp.result.unwrap()["protocolVersion"],
            crate::protocol::DEFAULT_PROTOCOL_VERSION
        );
    }

    #[tokio::test]
    async fn initialize_echoes_supported_client_version() {
        let svc = service(MockBackend::allowing());
        let resp = svc
            .handle(
                Some("token-a"),
                req(1, "initialize", json!({ "protocolVersion": "2024-11-05" })),
                0,
            )
            .await;
        assert_eq!(resp.result.unwrap()["protocolVersion"], "2024-11-05");
    }

    #[tokio::test]
    async fn initialize_with_unknown_version_offers_highest_supported() {
        // Spec-konform: kein harter Fehler, sondern „unser Bestes".
        let svc = service(MockBackend::allowing());
        let resp = svc
            .handle(
                Some("token-a"),
                req(1, "initialize", json!({ "protocolVersion": "2099-01-01" })),
                0,
            )
            .await;
        assert_eq!(
            resp.result.unwrap()["protocolVersion"],
            crate::protocol::highest_supported()
        );
    }

    #[tokio::test]
    async fn initialize_default_is_config_driven() {
        // Der Default ist ein Config-Flip: with_protocol_default steuert die
        // ausgehandelte Version für handshake-lose Clients.
        let svc = service_with_default(MockBackend::allowing(), "2024-11-05");
        let resp = svc
            .handle(Some("token-a"), req(1, "initialize", json!({})), 0)
            .await;
        assert_eq!(resp.result.unwrap()["protocolVersion"], "2024-11-05");
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

    #[test]
    fn call_log_line_carries_identity_and_provenance() {
        let claims = auth().verify("token-a").unwrap();
        let result = json!({
            "data": { "text": "Art. 1 BV" },
            "provenance": { "eli": "eli/cc/1999/404", "valid_as_of": "2020-01-01" }
        });
        let line = call_log_line(&claims, "read_article", &result, 42);
        let parsed: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["event"], "tools/call");
        assert_eq!(parsed["tool.name"], "read_article");
        assert_eq!(parsed["auth.tenant"], "kanzlei-a");
        assert_eq!(parsed["auth.session"], "sess-1");
        assert_eq!(parsed["auth.role"], "Reader");
        assert_eq!(parsed["provenance.eli"], "eli/cc/1999/404");
        assert_eq!(parsed["provenance.valid_as_of"], "2020-01-01");
        assert_eq!(parsed["outcome"], "ok");
        assert_eq!(parsed["span.duration_ms"], "42");
        // ADR-001. Response-Inhalte erscheinen nie in der Logzeile.
        assert!(!line.contains("Art. 1 BV"));
    }

    #[test]
    fn call_log_line_marks_graceful_errors() {
        let claims = auth().verify("token-a").unwrap();
        let result = json!({ "error": "unknown eli", "hint": "ELI pruefen" });
        let line = call_log_line(&claims, "read_article", &result, 5);
        let parsed: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["outcome"], "error");
        assert_eq!(parsed["provenance.eli"], "-");
        // Fehlertext (potenziell mit Nutzereingaben) bleibt draussen.
        assert!(!line.contains("unknown eli"));
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
    async fn discovery_call_books_weighted_quota_navigation_does_not() {
        // Registry mit einem Discovery- und einem Navigations-Tool. Beide werden
        // mit demselben gewichts-protokollierenden Backend aufgerufen.
        let mut registry = Registry::new();
        registry.register(Arc::new(FakeDiscovery));
        registry.register(Arc::new(ReadArticle));
        let backend = CostSpyBackend::new();
        let limiter = RateLimiter::with_policy(backend, QuotaPolicy::default());
        let temporal = TemporalResolver::new(time::macros::date!(2024 - 01 - 01));
        // Navigator-Rolle, damit Discovery sichtbar/aufrufbar ist (ADR-006).
        let nav_auth = StaticAuthResolver::new().with_credential(
            "token-nav",
            ClaimRecord {
                tenant: "kanzlei-a".into(),
                session: "sess-1".into(),
                role: Role::Navigator,
            },
        );
        let svc = McpService::new(registry, nav_auth, limiter, temporal);

        // Navigation bucht das Grundgewicht 1.
        svc.handle(
            Some("token-nav"),
            req(1, "tools/call", json!({ "name": "read_article" })),
            0,
        )
        .await;
        assert_eq!(
            svc.limiter.backend().last_cost.load(Ordering::SeqCst),
            1,
            "LocalNavigation muss Gewicht 1 buchen"
        );

        // Discovery bucht das höhere Pool-Gewicht (ADR-006).
        svc.handle(
            Some("token-nav"),
            req(2, "tools/call", json!({ "name": "search_law" })),
            0,
        )
        .await;
        assert_eq!(
            svc.limiter.backend().last_cost.load(Ordering::SeqCst),
            ToolPool::Discovery.cost_weight(),
            "Discovery muss das schwerere Pool-Gewicht buchen"
        );
        assert!(
            ToolPool::Discovery.cost_weight() > 1,
            "Discovery muss schwerer wiegen als Navigation"
        );
    }

    #[tokio::test]
    async fn jolux_metadata_call_books_weighted_quota_like_discovery() {
        // ADR-007: ein Metadaten-Call bucht dasselbe schwerere Live-Gewicht wie
        // Discovery, lokale Navigation bleibt bei 1.
        let mut registry = Registry::new();
        registry.register(Arc::new(FakeMetadata));
        registry.register(Arc::new(ReadArticle));
        let backend = CostSpyBackend::new();
        let limiter = RateLimiter::with_policy(backend, QuotaPolicy::default());
        let temporal = TemporalResolver::new(time::macros::date!(2024 - 01 - 01));
        let nav_auth = StaticAuthResolver::new().with_credential(
            "token-nav",
            ClaimRecord {
                tenant: "kanzlei-a".into(),
                session: "sess-1".into(),
                role: Role::Navigator,
            },
        );
        let svc = McpService::new(registry, nav_auth, limiter, temporal);

        // Lokale Navigation: Grundgewicht 1.
        svc.handle(
            Some("token-nav"),
            req(1, "tools/call", json!({ "name": "read_article" })),
            0,
        )
        .await;
        assert_eq!(
            svc.limiter.backend().last_cost.load(Ordering::SeqCst),
            1,
            "LocalNavigation muss Gewicht 1 buchen"
        );

        // JoluxMetadata: schwereres Live-Gewicht, identisch zu Discovery.
        svc.handle(
            Some("token-nav"),
            req(2, "tools/call", json!({ "name": "check_in_force" })),
            0,
        )
        .await;
        assert_eq!(
            svc.limiter.backend().last_cost.load(Ordering::SeqCst),
            ToolPool::JoluxMetadata.cost_weight(),
            "JoluxMetadata muss das schwerere Pool-Gewicht buchen"
        );
        assert_eq!(
            ToolPool::JoluxMetadata.cost_weight(),
            ToolPool::Discovery.cost_weight(),
            "JoluxMetadata muss exakt das Discovery-Gewicht buchen"
        );
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

    /// Invarianz nach dem Phase-3.2-Vorab-Umbau (`Json<…>` → [`Response`]).
    ///
    /// Der Rückgabetyp ist jetzt allgemeiner, das **beobachtbare Verhalten muss
    /// aber bit-identisch** bleiben, solange noch keine Status-Code-Logik (3.x/5.x)
    /// existiert: jeder Pfad antwortet **HTTP 200** mit
    /// `content-type: application/json` — auch der Auth-Fehlerfall, der nur als
    /// JSON-RPC-Error im Body erscheint, nie als HTTP-Status. Dieser Test fällt
    /// rot, sobald jemand versehentlich einen abweichenden Status einführt, und
    /// schützt damit die Alt-Clients (ansV, syllogismus-fedlex), die ausschliesslich
    /// den Body auswerten.
    #[tokio::test]
    async fn rpc_handler_keeps_200_json_for_all_paths() {
        use axum::body::Body;
        use axum::http::{header::CONTENT_TYPE, Request, StatusCode};
        use tower::ServiceExt;

        let svc = Arc::new(service(MockBackend::allowing()));
        let app = router(svc);

        // Drei repräsentative Pfade, die intern unterschiedlich enden:
        //  - Parse-Fehler (kein gültiges JSON),
        //  - Auth-Fehler (gültiges JSON, kein Token),
        //  - Erfolg (gültiges JSON + Token).
        let cases: Vec<(&str, Body)> = vec![
            ("parse error", Body::from("{ this is not json")),
            (
                "auth error",
                Body::from(
                    serde_json::to_vec(&json!({
                        "jsonrpc": "2.0", "id": 1, "method": "tools/list"
                    }))
                    .unwrap(),
                ),
            ),
        ];

        for (label, body) in cases {
            let resp = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/rpc")
                        .header("content-type", "application/json")
                        .body(body)
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::OK,
                "{label}: muss HTTP 200 bleiben (Status-Logik kommt erst in Phase 3.x/5.x)"
            );
            let ct = resp
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_string();
            assert!(
                ct.starts_with("application/json"),
                "{label}: content-type muss application/json bleiben, war `{ct}`"
            );
            // Body bleibt eine valide JSON-RPC-Hülle.
            let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap();
            let v: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(v["jsonrpc"], "2.0", "{label}: jsonrpc-Marker bleibt");
        }
    }
}

