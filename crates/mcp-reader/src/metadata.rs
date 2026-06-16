//! JOLux-Metadaten-Tools des MCP-Readers (Pool::JoluxMetadata, ADR-007).
//!
//! Tranche A — Temporal. Drei dünne Hüllen um die norm-tragenden Primitive aus
//! `fedlex-jolux`: `check_in_force` (JLX-TMP-03), `list_versions` (JLX-TMP-01)
//! und `resolve_consolidation_at` (JLX-TMP-02). Sie beantworten die
//! gutachtenkritischen Fragen „Gilt die Norm zum Stichtag?", „Welche Fassungen
//! gibt es?" und „Welche Fassung galt zum Stichtag (mit XML-URL)?".
//!
//! ## Norm-Provenance statt Hinweis (ADR-007, im Gegensatz zu Discovery)
//!
//! Anders als die Discovery-Tools (ADR-006) lesen diese Tools eine Eigenschaft
//! eines **bekannten** Erlasses (Eingabe ist ein ELI), nicht einen Suchtreffer.
//! Jedes Primitiv liefert bereits ein [`Response<T>`] mit `Provenance::new(...)`
//! (`kind: "norm"`); die Hülle reicht diese Herkunft unverändert durch. Das
//! ADR-004-Gate ist damit schon in der `fedlex-jolux`-Schicht erfüllt.
//!
//! ## Stichtag aus dem Kontext, nicht aus den Argumenten
//!
//! Wie bei den Navigations-Tools kommt der Stichtag NIE aus den Tool-Argumenten,
//! sondern immer aus dem [`ToolContext`]-Stempel des Temporal Resolvers
//! ([`ctx.stamp.valid_as_of()`]). So kann ein Tool den Stichtag nicht
//! verfälschen, gegen den der Rest der Anfrage gestellt wurde.

use crate::registry::Registry;
use crate::tool::{McpTool, ToolContext, ToolError, ToolPool};
use async_trait::async_trait;
use fedlex_core::{Eli, Response};
use fedlex_jolux::{
    check_in_force, list_versions, resolve_consolidation_at, JoluxError, Language, SparqlClient,
};
use serde_json::{json, Value};
use std::sync::Arc;

/// Registriert alle JOLux-Metadaten-Tools (Tranche A) an der Registry.
///
/// Sie teilen sich einen SPARQL-Client (Live-Auflösung gegen Fedlex). Wie die
/// Discovery-Tools brauchen sie keinen AKN-Fetcher — sie lesen Metadaten, kein
/// Dokument.
pub fn register_metadata_tools<C>(registry: &mut Registry, client: Arc<C>)
where
    C: SparqlClient + Send + Sync + 'static,
{
    registry.register(Arc::new(CheckInForce {
        client: Arc::clone(&client),
    }));
    registry.register(Arc::new(ListVersions {
        client: Arc::clone(&client),
    }));
    registry.register(Arc::new(ResolveConsolidationAt { client }));
}

// ---------------------------------------------------------------------------
// Argument-Parsing & Fehler-Mapping (gespiegelt aus discovery.rs)
// ---------------------------------------------------------------------------

/// Pflicht-Argument `eli` als validiertes [`Eli`] lesen.
fn arg_eli(args: &Value) -> Result<Eli, ToolError> {
    let raw = args
        .get("eli")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidArguments("`eli` (string) fehlt".into()))?;
    Eli::new(raw).map_err(|e| ToolError::InvalidArguments(e.to_string()))
}

/// Optionales Argument `lang` lesen (Default Deutsch).
fn arg_lang(args: &Value) -> Result<Language, ToolError> {
    match args.get("lang").and_then(Value::as_str) {
        None | Some("de") => Ok(Language::De),
        Some("fr") => Ok(Language::Fr),
        Some("it") => Ok(Language::It),
        Some("en") => Ok(Language::En),
        Some("rm") | Some("roh") => Ok(Language::Roh),
        Some(other) => Err(ToolError::InvalidArguments(format!(
            "`lang` muss de|fr|it|en|rm sein, nicht `{other}`"
        ))),
    }
}

/// JOLux-Fehler in lenkende Tool-Fehler übersetzen.
fn map_jolux(err: JoluxError) -> ToolError {
    match err {
        JoluxError::NotFound(what) => ToolError::NotFound(what),
        other => ToolError::Upstream(other.to_string()),
    }
}

/// Wandelt ein norm-tragendes `Response<T>` des Primitivs in ein
/// `Response<Value>` — die Provenance bleibt erhalten (ADR-004/ADR-007).
fn into_value_response<T: serde::Serialize>(
    resp: Response<T>,
) -> Result<Response<Value>, ToolError> {
    let (data, prov) = resp.into_parts();
    let value =
        serde_json::to_value(data).map_err(|e| ToolError::Upstream(format!("serialize: {e}")))?;
    Ok(Response::new(value, prov))
}

// ---------------------------------------------------------------------------
// Die Tools
// ---------------------------------------------------------------------------

/// JLX-TMP-03. Prüft, ob ein Erlass zum Stichtag der Anfrage in Kraft ist.
struct CheckInForce<C> {
    client: Arc<C>,
}

#[async_trait]
impl<C> McpTool for CheckInForce<C>
where
    C: SparqlClient + Send + Sync,
{
    fn name(&self) -> &str {
        "check_in_force"
    }
    fn pool(&self) -> ToolPool {
        ToolPool::JoluxMetadata
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Prueft, ob ein Erlass zum Stichtag der Anfrage in Kraft ist (JLX-TMP-03). Doppel-Logik: primaer ueber Datumsfelder, Fallback auf Status-Vokabular. Liefert einen BELEG (kind=norm) ueber den genannten Erlass.",
            "properties": {
                "eli": { "type": "string", "description": "ELI des Erlasses, z.B. eli/cc/2017/762" }
            },
            "required": ["eli"]
        })
    }
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Response<Value>, ToolError> {
        let eli = arg_eli(&args)?;
        let resp = check_in_force(self.client.as_ref(), &eli, ctx.stamp.valid_as_of())
            .await
            .map_err(map_jolux)?;
        into_value_response(resp)
    }
}

/// JLX-TMP-01. Listet alle Fassungen (Consolidations) eines Erlasses.
struct ListVersions<C> {
    client: Arc<C>,
}

#[async_trait]
impl<C> McpTool for ListVersions<C>
where
    C: SparqlClient + Send + Sync,
{
    fn name(&self) -> &str {
        "list_versions"
    }
    fn pool(&self) -> ToolPool {
        ToolPool::JoluxMetadata
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Listet alle Fassungen (Consolidations) eines Erlasses chronologisch (JLX-TMP-01). Leere Liste, wenn der Erlass keine Consolidations hat (kein Fehler). Liefert einen BELEG (kind=norm) ueber den genannten Erlass.",
            "properties": {
                "eli": { "type": "string", "description": "ELI des Erlasses, z.B. eli/cc/2017/762" }
            },
            "required": ["eli"]
        })
    }
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Response<Value>, ToolError> {
        let eli = arg_eli(&args)?;
        let resp = list_versions(self.client.as_ref(), &eli, ctx.stamp.valid_as_of())
            .await
            .map_err(map_jolux)?;
        into_value_response(resp)
    }
}

/// JLX-TMP-02. Löst die zum Stichtag gültige konsolidierte Fassung auf.
struct ResolveConsolidationAt<C> {
    client: Arc<C>,
}

#[async_trait]
impl<C> McpTool for ResolveConsolidationAt<C>
where
    C: SparqlClient + Send + Sync,
{
    fn name(&self) -> &str {
        "resolve_consolidation_at"
    }
    fn pool(&self) -> ToolPool {
        ToolPool::JoluxMetadata
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Loest die zum Stichtag der Anfrage gueltige konsolidierte Fassung eines Erlasses auf, inkl. XML-URL (JLX-TMP-02). Vermeidet den Fehler 'immer die neueste Fassung'. NotFound, wenn es zum Stichtag keine Fassung gibt. Liefert einen BELEG (kind=norm).",
            "properties": {
                "eli": { "type": "string", "description": "ELI des Erlasses, z.B. eli/cc/2017/762" },
                "lang": { "type": "string", "enum": ["de", "fr", "it", "en", "rm"], "default": "de" }
            },
            "required": ["eli"]
        })
    }
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Response<Value>, ToolError> {
        let eli = arg_eli(&args)?;
        let lang = arg_lang(&args)?;
        let resp =
            resolve_consolidation_at(self.client.as_ref(), &eli, ctx.stamp.valid_as_of(), lang)
                .await
                .map_err(map_jolux)?;
        into_value_response(resp)
    }
}

// ---------------------------------------------------------------------------
// Tests — MockSparqlClient, kein Netzwerk.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthResolver, ClaimRecord, Role, StaticAuthResolver};
    use crate::temporal::TemporalResolver;
    use fedlex_core::TransactionTime;
    use fedlex_jolux::MockSparqlClient;
    use time::macros::{date, datetime};

    /// Canned in-force-Prüfung: in Kraft seit 2018, kein Ausserkrafttreten.
    const IN_FORCE_JSON: &str = r#"{
      "head": { "vars": ["status","entry","noLonger","endApp"] },
      "results": { "bindings": [{
        "status": { "type": "uri", "value": "https://fedlex.data.admin.ch/vocabulary/enforcement-status/0" },
        "entry": { "type": "literal", "value": "2018-01-01" }
      }] }
    }"#;

    /// Canned Versionsliste: zwei Fassungen.
    const VERSIONS_JSON: &str = r#"{
      "head": { "vars": ["cons","date"] },
      "results": { "bindings": [
        { "cons": { "type": "uri", "value": "https://fedlex.data.admin.ch/eli/cc/2017/762/20180101" },
          "date": { "type": "literal", "value": "2018-01-01" } },
        { "cons": { "type": "uri", "value": "https://fedlex.data.admin.ch/eli/cc/2017/762/20210101" },
          "date": { "type": "literal", "value": "2021-01-01" } }
      ] }
    }"#;

    /// Canned Consolidation-Auflösung: eine Fassung + XML-URL.
    const CONS_JSON: &str = r#"{
      "head": { "vars": ["cons","date","url"] },
      "results": { "bindings": [{
        "cons": { "type": "uri", "value": "https://fedlex.data.admin.ch/eli/cc/2017/762/20210101" },
        "date": { "type": "literal", "value": "2021-01-01" },
        "url": { "type": "uri", "value": "https://fedlex.data.admin.ch/eli/cc/2017/762/20210101/de/xml" }
      }] }
    }"#;

    fn registry_with(json: &str) -> Registry {
        let client = Arc::new(MockSparqlClient::from_json(json));
        let mut r = Registry::new();
        register_metadata_tools(&mut r, client);
        r
    }

    fn ctx(role: Role) -> ToolContext {
        let claims = StaticAuthResolver::new()
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
        let stamp = TemporalResolver::new(date!(2026 - 06 - 01))
            .stamp_at(None, TransactionTime::new(datetime!(2026-06-10 09:00 UTC)));
        ToolContext { claims, stamp }
    }

    #[tokio::test]
    async fn metadata_tools_hidden_from_reader_visible_to_navigator() {
        let r = registry_with(IN_FORCE_JSON);

        let reader: Vec<String> = r
            .list_tools(Role::Reader)
            .into_iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert!(
            reader.is_empty(),
            "Reader darf KEIN JoluxMetadata sehen, sah: {reader:?}"
        );

        let nav: Vec<String> = r
            .list_tools(Role::Navigator)
            .into_iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        for expected in [
            "check_in_force",
            "list_versions",
            "resolve_consolidation_at",
        ] {
            assert!(
                nav.contains(&expected.to_string()),
                "Navigator fehlt: {expected}"
            );
        }
    }

    #[tokio::test]
    async fn reader_call_on_check_in_force_is_gracefully_denied() {
        let r = registry_with(IN_FORCE_JSON);
        let out = r
            .dispatch(
                &ctx(Role::Reader),
                "check_in_force",
                json!({ "eli": "eli/cc/2017/762" }),
            )
            .await;
        assert!(
            out["error"].as_str().unwrap().contains("not permitted"),
            "war: {out}"
        );
    }

    #[tokio::test]
    async fn check_in_force_carries_norm_provenance_for_requested_eli() {
        let r = registry_with(IN_FORCE_JSON);
        let out = r
            .dispatch(
                &ctx(Role::Navigator),
                "check_in_force",
                json!({ "eli": "eli/cc/2017/762" }),
            )
            .await;
        assert!(out.get("error").is_none(), "unerwarteter Fehler: {out}");
        // ADR-007: BELEG, kein Hinweis — und auf das angefragte ELI bezogen.
        assert_eq!(out["provenance"]["kind"], "norm");
        assert_eq!(out["provenance"]["eli"], "eli/cc/2017/762");
        assert_eq!(out["data"]["in_force"], true);
    }

    #[tokio::test]
    async fn list_versions_returns_chronological_versions_with_norm_provenance() {
        let r = registry_with(VERSIONS_JSON);
        let out = r
            .dispatch(
                &ctx(Role::Navigator),
                "list_versions",
                json!({ "eli": "eli/cc/2017/762" }),
            )
            .await;
        let versions = out["data"].as_array().expect("versions-Array");
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0]["date_applicability"], "2018-01-01");
        assert_eq!(out["provenance"]["kind"], "norm");
        assert_eq!(out["provenance"]["eli"], "eli/cc/2017/762");
    }

    #[tokio::test]
    async fn resolve_consolidation_at_yields_xml_url_with_norm_provenance() {
        let r = registry_with(CONS_JSON);
        let out = r
            .dispatch(
                &ctx(Role::Navigator),
                "resolve_consolidation_at",
                json!({ "eli": "eli/cc/2017/762" }),
            )
            .await;
        assert!(out.get("error").is_none(), "unerwarteter Fehler: {out}");
        assert_eq!(
            out["data"]["xml_url"],
            "https://fedlex.data.admin.ch/eli/cc/2017/762/20210101/de/xml"
        );
        assert_eq!(out["provenance"]["kind"], "norm");
        assert_eq!(out["provenance"]["eli"], "eli/cc/2017/762");
    }

    #[tokio::test]
    async fn missing_eli_is_invalid_arguments() {
        let r = registry_with(IN_FORCE_JSON);
        let out = r
            .dispatch(&ctx(Role::Navigator), "check_in_force", json!({}))
            .await;
        assert!(out["error"].as_str().unwrap().contains("invalid arguments"));
    }

    #[tokio::test]
    async fn consolidation_not_found_is_graceful_not_found() {
        let empty =
            r#"{ "head": { "vars": ["cons","date","url"] }, "results": { "bindings": [] } }"#;
        let r = registry_with(empty);
        let out = r
            .dispatch(
                &ctx(Role::Navigator),
                "resolve_consolidation_at",
                json!({ "eli": "eli/cc/2017/762" }),
            )
            .await;
        assert!(out["error"].as_str().unwrap().contains("not found"));
    }
}
