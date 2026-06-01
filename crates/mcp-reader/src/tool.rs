//! MCP-Tool-Abstraktion mit strukturellem Provenance-Gate (ADR-004).
//!
//! Der Kern der Entscheidung. [`McpTool::execute`] gibt ausschliesslich ein
//! [`Response<Value>`] zurück. Da `Response` nur mit einer `Provenance`
//! konstruierbar ist (privates Feld in `fedlex-core`), kann KEIN Tool eine
//! Antwort ohne Herkunft liefern. Das Gate ist damit eine Compile-Zeit-Garantie,
//! kein Review-Kommentar.
//!
//! RBAC-Pools setzen Least-Privilege um. Jedes Tool gehört zu genau einem
//! [`ToolPool`], und eine Rolle sieht nur die Pools, die ihr zustehen.

use crate::auth::{Role, VerifiedClaims};
use crate::temporal::QueryStamp;
use async_trait::async_trait;
use fedlex_core::Response;
use serde_json::Value;

/// Least-Privilege-Gruppen der MCP-Oberfläche.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolPool {
    /// Navigation im lokalen Cache (AKN-Bäume lesen).
    LocalNavigation,
    /// Föderierte URI-Auflösung über Kantone/Gemeinden/Ausland.
    LodFederation,
    /// Schema-/Konsistenz-Validierung und XML-Diffing.
    Validation,
    /// Stateful Workspace-Tools (Scratchpad).
    Workspace,
}

/// Welche Pools eine Rolle sehen darf (RBAC, Least-Privilege).
pub fn pools_for(role: Role) -> &'static [ToolPool] {
    match role {
        Role::Reader => &[ToolPool::LocalNavigation],
        Role::Navigator => &[
            ToolPool::LocalNavigation,
            ToolPool::LodFederation,
            ToolPool::Workspace,
        ],
        Role::Validator => &[
            ToolPool::LocalNavigation,
            ToolPool::LodFederation,
            ToolPool::Workspace,
            ToolPool::Validation,
        ],
    }
}

/// Ob eine Rolle einen Pool nutzen darf.
pub fn role_allows(role: Role, pool: ToolPool) -> bool {
    pools_for(role).contains(&pool)
}

/// Ausführungskontext eines Tool-Aufrufs. Bündelt den server-validierten Claim
/// und den Anfrage-Stempel des Temporal Resolvers.
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Geprüfte Identität (Mandant, Session, Rolle).
    pub claims: VerifiedClaims,
    /// Bi-temporaler Stempel der Anfrage.
    pub stamp: QueryStamp,
}

/// Fehler eines Tool-Aufrufs. Wird von der Graceful-Failure-Middleware in eine
/// lenkende `{ error, hint }`-Antwort für das LLM übersetzt, nie als Crash.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// Die Argumente des Aufrufs waren ungültig.
    #[error("invalid arguments: {0}")]
    InvalidArguments(String),
    /// Die angefragte Ressource existiert nicht.
    #[error("not found: {0}")]
    NotFound(String),
    /// Ein nachgelagerter Dienst (Cache, LOD, Semantic) versagte.
    #[error("upstream failure: {0}")]
    Upstream(String),
}

impl ToolError {
    /// Ein lenkender Hinweis für das LLM, wie es weitermachen kann.
    pub fn hint(&self) -> &'static str {
        match self {
            ToolError::InvalidArguments(_) => {
                "Pruefe die Argumente gegen das tool-Schema und versuche es erneut."
            }
            ToolError::NotFound(_) => {
                "Die Ressource existiert nicht. Pruefe ELI/Stichtag oder nutze ein Suchtool."
            }
            ToolError::Upstream(_) => {
                "Ein nachgelagerter Dienst ist momentan nicht verfuegbar. Versuche es spaeter erneut."
            }
        }
    }
}

/// Ein einzelnes MCP-Tool.
///
/// `execute` MUSS ein [`Response<Value>`] liefern. Damit ist die Herkunft jeder
/// erfolgreichen Antwort strukturell erzwungen (ADR-004).
#[async_trait]
pub trait McpTool: Send + Sync {
    /// Eindeutiger Tool-Name (`tools/call`-Schlüssel).
    fn name(&self) -> &str;

    /// Der RBAC-Pool, zu dem dieses Tool gehört.
    fn pool(&self) -> ToolPool;

    /// JSON-Schema der Argumente (für `tools/list`).
    fn schema(&self) -> Value;

    /// Führt das Tool aus. Erfolg trägt zwingend Provenance.
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Response<Value>, ToolError>;
}
