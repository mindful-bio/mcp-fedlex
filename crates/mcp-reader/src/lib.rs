//! mcp-reader - zustandsloser MCP-Reader (CQRS-Leseseite).
//!
//! Ab M3. Die Eingangstür mit Auth/RBAC ([`auth`]) und dem verteilten,
//! fail-closed gedrosselten Quota ([`quota`], ADR-002). Ab M4 die MCP-Registry
//! ([`registry`]) mit Tool-Pools, Temporal Resolver ([`temporal`]) und dem
//! strukturellen Provenance-Gate ([`tool`], ADR-004). Ab M5 die XML/AKN-Engine
//! ([`xml_engine`]) mit L1-Cache und Paginierung sowie die Execution-Sandbox
//! ([`sandbox`]) mit harter Deadline gegen halluzinierte DoS-Queries. Ab M6 das
//! LOD-Gateway ([`lod_gateway`]) mit Lokal-vs-Extern-Auflösung, geschützt durch
//! den [`circuit_breaker`]. Ab M7 der dünne, optionale Semantic-Client
//! ([`semantic_client`]) mit Graceful Degradation und Provenance je Treffer. Der
//! SSE/JSON-RPC-Transport ([`transport`]) verdrahtet alles zur Eingangstür. Ab
//! M10 die Betriebs-Health-Endpunkte ([`health`]) mit getrennter Liveness,
//! Readiness und Startup sowie der Cache-Warmup ([`warmup`]) mit Single-Flight
//! gegen Stampede.

#![forbid(unsafe_code)]

pub mod auth;
pub mod circuit_breaker;
pub mod health;
pub mod lod_gateway;
pub mod quota;
pub mod registry;
pub mod sandbox;
pub mod semantic_client;
pub mod temporal;
pub mod tool;
pub mod transport;
pub mod warmup;
pub mod xml_engine;

pub use auth::{AuthError, AuthResolver, ClaimRecord, Role, StaticAuthResolver, VerifiedClaims};
pub use circuit_breaker::{BreakerConfig, BreakerError, BreakerState, CircuitBreaker};
pub use health::{health_router, HealthState, ReadinessProbe};
pub use lod_gateway::{
    ConnectorError, EliResolver, ExternalConnector, Origin, ResolveError, Resolved,
};
pub use quota::{Decision, QuotaBackend, QuotaError, QuotaPolicy, RateLimiter, RedisQuotaBackend};
pub use registry::Registry;
pub use sandbox::{SandboxError, SearchSandbox};
pub use semantic_client::{
    BackendError, RawHit, ScoredHit, SearchOutcome, SemanticBackend, SemanticClient,
};
pub use temporal::{QueryStamp, TemporalResolver};
pub use tool::{pools_for, role_allows, McpTool, ToolContext, ToolError, ToolPool};
pub use transport::{router, JsonRpcError, JsonRpcRequest, JsonRpcResponse, McpService};
pub use warmup::{WarmupCache, WarmupReport};
pub use xml_engine::{diff_to_markdown, paginate, Article, Document, L1Cache, Page};
