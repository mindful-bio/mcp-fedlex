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
    /// Live-Discovery gegen Fedlex (Suche/SR-Auflösung/Themen, ADR-006).
    ///
    /// Eigener Pool, getrennt von [`Self::LodFederation`]: Föderation löst
    /// *bekannte* URIs auf, Discovery *findet* erst einen Bund-Erlass. Discovery-
    /// Treffer tragen Hinweis-Provenance (kein Beleg) und gehen live an den
    /// öffentlichen Endpoint — daher wiegt der Pool im Quota schwerer.
    Discovery,
    /// Live-JOLux-Metadaten gegen Fedlex (Temporal/Beziehungen/Einordnung, ADR-007).
    ///
    /// Eigener Pool, getrennt von [`Self::Discovery`] und [`Self::LocalNavigation`]:
    /// Discovery *findet* einen Erlass (Hinweis), diese Tools *belegen* eine
    /// Eigenschaft eines **bekannten** Erlasses (Norm-Provenance) und gehen — wie
    /// Discovery, anders als der Cache-gestützte [`Self::LocalNavigation`] — live an
    /// den öffentlichen Endpoint. Daher dasselbe schwerere Quota-Gewicht.
    JoluxMetadata,
    /// Schema-/Konsistenz-Validierung und XML-Diffing.
    Validation,

    /// Stateful Workspace-Tools (Scratchpad).
    Workspace,
}

impl ToolPool {
    /// Quota-Cost-Gewicht des Pools (ADR-006). Live-Discovery wiegt schwerer
    /// als lokale Navigation, um den öffentlichen Fedlex-Endpoint vor
    /// LLM-Schleifen zu schützen. Claim-gebunden, von keinem LLM-Parameter
    /// senkbar (ADR-002).
    pub fn cost_weight(self) -> u32 {
        match self {
            // Live-Last gegen den öffentlichen Fedlex-Endpoint (ADR-006/ADR-007).
            ToolPool::Discovery | ToolPool::JoluxMetadata => 5,
            _ => 1,
        }
    }
}

/// Welche Pools eine Rolle sehen darf (RBAC, Least-Privilege).
pub fn pools_for(role: Role) -> &'static [ToolPool] {
    match role {
        // Reader bleibt eng: nur lokaler Cache, KEIN Discovery (ADR-006).
        Role::Reader => &[ToolPool::LocalNavigation],
        // Navigator ist die Rolle, mit der ansV läuft: Discovery erlaubt.
        Role::Navigator => &[
            ToolPool::LocalNavigation,
            ToolPool::LodFederation,
            ToolPool::Discovery,
            ToolPool::JoluxMetadata,
            ToolPool::Workspace,
        ],
        Role::Validator => &[
            ToolPool::LocalNavigation,
            ToolPool::LodFederation,
            ToolPool::Discovery,
            ToolPool::JoluxMetadata,
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

#[cfg(test)]
mod tests {
    use super::*;

    // ADR-007: die Live-SPARQL-Pools wiegen im Quota schwerer als lokale
    // Navigation — und JoluxMetadata trägt exakt das Discovery-Gewicht
    // (gleiches Lastprofil gegen den öffentlichen Fedlex-Endpunkt).
    #[test]
    fn jolux_metadata_costs_same_as_discovery_and_more_than_local() {
        assert_eq!(
            ToolPool::JoluxMetadata.cost_weight(),
            ToolPool::Discovery.cost_weight(),
            "JoluxMetadata muss dasselbe Live-Gewicht wie Discovery tragen"
        );
        assert!(
            ToolPool::JoluxMetadata.cost_weight() > ToolPool::LocalNavigation.cost_weight(),
            "Live-SPARQL muss schwerer wiegen als lokale Navigation"
        );
    }

    // ADR-007: Sichtbarkeit folgt Least-Privilege — Reader bleibt eng auf den
    // lokalen Cache, Navigator/Validator dürfen die Metadaten-Tools sehen.
    #[test]
    fn jolux_metadata_visibility_follows_least_privilege() {
        assert!(!role_allows(Role::Reader, ToolPool::JoluxMetadata));
        assert!(role_allows(Role::Navigator, ToolPool::JoluxMetadata));
        assert!(role_allows(Role::Validator, ToolPool::JoluxMetadata));
    }
}
