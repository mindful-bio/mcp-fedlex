//! MCP-Registry. Tool-Pool-Verwaltung, RBAC-gefiltertes `tools/list`, Dispatch
//! und die globale Graceful-Failure-Middleware (ADR-004).
//!
//! Zwei Garantien dieser Schicht.
//! 1. `tools/list` liefert pro Rolle NUR den erlaubten Pool (Least-Privilege).
//! 2. `dispatch` gibt IMMER valides JSON zurück. Erfolg ist ein
//!    `{ data, provenance }` aus [`Response`] (Provenance strukturell erzwungen),
//!    jeder Fehler wird zu einem lenkenden `{ error, hint }`. Niemals ein
//!    roher Crash ans LLM.

use crate::auth::Role;
use crate::tool::{role_allows, McpTool, ToolContext, ToolPool};

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Zentrale Registry der MCP-Tools.
#[derive(Default, Clone)]
pub struct Registry {
    tools: BTreeMap<String, Arc<dyn McpTool>>,
}

impl Registry {
    /// Erzeugt eine leere Registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registriert ein Tool. Ein gleichnamiges Tool wird überschrieben.
    pub fn register(&mut self, tool: Arc<dyn McpTool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Der RBAC-Pool eines registrierten Tools, falls bekannt. Wird vom
    /// Transport gebraucht, um das pool-abhängige Quota-Gewicht (ADR-006)
    /// VOR dem Dispatch zu bestimmen.
    pub fn pool_of(&self, name: &str) -> Option<ToolPool> {
        self.tools.get(name).map(|t| t.pool())
    }

    /// `tools/list` für eine Rolle. Liefert nur Schemas der erlaubten Pools.
    pub fn list_tools(&self, role: Role) -> Vec<Value> {
        self.tools
            .values()
            .filter(|t| role_allows(role, t.pool()))
            .map(|t| {
                json!({
                    "name": t.name(),
                    "schema": t.schema(),
                })
            })
            .collect()
    }

    /// `tools/call`. Führt ein Tool aus und liefert immer valides JSON.
    ///
    /// Reihenfolge der Prüfungen. Existenz, dann RBAC, dann Ausführung. Jeder
    /// Fehlerpfad endet in der Graceful-Failure-Hülle.
    pub async fn dispatch(&self, ctx: &ToolContext, name: &str, args: Value) -> Value {
        let role = ctx.claims.role();

        let Some(tool) = self.tools.get(name) else {
            return graceful(
                &format!("unknown tool `{name}`"),
                "Rufe zuerst tools/list auf und waehle einen verfuegbaren Tool-Namen.",
            );
        };

        // Least-Privilege. Eine Rolle darf fremde Pools nicht aufrufen, selbst
        // wenn sie den Namen errät.
        if !role_allows(role, tool.pool()) {
            return graceful(
                &format!("tool `{name}` not permitted for this role"),
                "Dieses Tool steht deiner Rolle nicht zur Verfuegung.",
            );
        }

        match tool.execute(ctx, args).await {
            // Erfolg. Die Serialisierung von Response enthält zwingend die
            // Provenance. Das ist der strukturelle ADR-004-Nachweis im Wire-Format.
            Ok(response) => match serde_json::to_value(&response) {
                Ok(value) => value,
                Err(e) => graceful(
                    &format!("serialization failed: {e}"),
                    "Interner Fehler beim Verpacken der Antwort.",
                ),
            },
            Err(err) => graceful(&err.to_string(), err.hint()),
        }
    }
}

/// Baut die lenkende Fehler-Hülle für das LLM.
fn graceful(error: &str, hint: &str) -> Value {
    json!({ "error": error, "hint": hint })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthResolver, ClaimRecord, StaticAuthResolver, VerifiedClaims};
    use crate::temporal::TemporalResolver;
    use crate::tool::{ToolError, ToolPool};
    use async_trait::async_trait;
    use fedlex_core::{Eli, Response, TransactionTime};
    use serde_json::json;
    use time::macros::{date, datetime};

    fn ctx(role: Role) -> ToolContext {
        let claims: VerifiedClaims = StaticAuthResolver::new()
            .with_credential(
                "c",
                ClaimRecord {
                    tenant: "kanzlei-a".into(),
                    session: "sess-1".into(),
                    role,
                },
            )
            .verify("c")
            .unwrap();
        let stamp = TemporalResolver::new(date!(2024 - 01 - 01)).stamp_at(
            Some(date!(2020 - 01 - 01)),
            TransactionTime::new(datetime!(2026-06-01 09:00 UTC)),
        );
        ToolContext { claims, stamp }
    }

    /// Ein wohlerzogenes Tool. Baut die Provenance aus dem Anfrage-Stempel.
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
            json!({ "type": "object", "properties": { "eli": { "type": "string" } } })
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

    /// Ein Validierungs-Tool. Nur für Validator-Rolle sichtbar.
    struct ValidateSchema;

    #[async_trait]
    impl McpTool for ValidateSchema {
        fn name(&self) -> &str {
            "validate_schema"
        }
        fn pool(&self) -> ToolPool {
            ToolPool::Validation
        }
        fn schema(&self) -> Value {
            json!({ "type": "object" })
        }
        async fn execute(
            &self,
            _ctx: &ToolContext,
            _args: Value,
        ) -> Result<Response<Value>, ToolError> {
            Err(ToolError::Upstream("validator offline".into()))
        }
    }

    fn registry() -> Registry {
        let mut r = Registry::new();
        r.register(Arc::new(ReadArticle));
        r.register(Arc::new(ValidateSchema));
        r
    }

    #[tokio::test]
    async fn list_tools_is_role_filtered_least_privilege() {
        let r = registry();

        let reader: Vec<_> = r
            .list_tools(Role::Reader)
            .into_iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(reader, vec!["read_article"]); // kein validate_schema

        let validator: Vec<_> = r
            .list_tools(Role::Validator)
            .into_iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert!(validator.contains(&"read_article".to_string()));
        assert!(validator.contains(&"validate_schema".to_string()));
    }

    #[tokio::test]
    async fn successful_dispatch_carries_provenance_in_wire_format() {
        let r = registry();
        let out = r
            .dispatch(&ctx(Role::Reader), "read_article", json!({}))
            .await;

        // ADR-004. Die Antwort trägt strukturell ihre Herkunft.
        assert_eq!(out["data"]["text"], "Art. 1 BV");
        assert_eq!(out["provenance"]["eli"], "eli/cc/1999/404");
        assert!(
            !out["provenance"]["valid_as_of"].is_null(),
            "valid_as_of muss in der Herkunft gesetzt sein"
        );
        assert!(
            !out["provenance"]["transaction_time"].is_null(),
            "transaction_time muss in der Herkunft gesetzt sein"
        );
        assert!(out.get("error").is_none());
    }

    #[tokio::test]
    async fn unknown_tool_yields_graceful_failure_not_panic() {
        let r = registry();
        let out = r
            .dispatch(&ctx(Role::Reader), "drop_database", json!({}))
            .await;
        assert!(out["error"].as_str().unwrap().contains("unknown tool"));
        assert!(out["hint"].is_string());
    }

    #[tokio::test]
    async fn forbidden_pool_is_denied_for_role() {
        let r = registry();
        // Reader kennt validate_schema nicht und darf es auch nicht aufrufen.
        let out = r
            .dispatch(&ctx(Role::Reader), "validate_schema", json!({}))
            .await;
        assert!(out["error"].as_str().unwrap().contains("not permitted"));
    }

    #[tokio::test]
    async fn tool_error_is_translated_to_hint() {
        let r = registry();
        // Validator darf validate_schema, das Tool meldet aber Upstream-Fehler.
        let out = r
            .dispatch(&ctx(Role::Validator), "validate_schema", json!({}))
            .await;
        assert!(out["error"].as_str().unwrap().contains("upstream failure"));
        assert!(out["hint"].as_str().unwrap().contains("nachgelagerter"));
    }
}
