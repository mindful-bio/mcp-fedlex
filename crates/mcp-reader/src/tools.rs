//! Die produktiven Navigations-Tools des MCP-Readers (Pool::LocalNavigation).
//!
//! Jedes Tool ist eine dünne Hülle um genau ein AKN-Primitiv aus `fedlex-akn`,
//! gespeist durch den [`AknFetcher`] aus `fedlex-bridge` (Direct Fetch gegen
//! Fedlex, gecached pro Manifestations-URL). Der Stichtag kommt NIE aus den
//! Tool-Argumenten, sondern immer aus dem [`ToolContext`]-Stempel des Temporal
//! Resolvers — so können Tools den Stichtag nicht untereinander verfälschen.
//!
//! Provenance (ADR-004). Wo das Primitiv selbst ein [`Response`] liefert
//! (TXT-01/02/03, STR-01), gilt dessen Herkunft, denn sie stammt strukturell
//! aus dem FRBR-Block des geparsten Dokuments. Wo das Primitiv nackt ist
//! (TXT-04, DOC-02/03), gilt die Herkunft der JOLux-Auflösung der Bridge.

use crate::tool::{McpTool, ToolContext, ToolError, ToolPool};
use async_trait::async_trait;
use fedlex_akn::{
    classify_pattern, get_all_references, get_article_text, get_document_structure,
    get_element_text, get_frbr_metadata, get_modifications, get_readable_document, search_text,
    AknDocument,
};
use fedlex_bridge::{AknFetcher, BridgeError, XmlSource};
use fedlex_core::{Eli, Response, ValidAsOf};
use fedlex_jolux::{JoluxError, Language, SparqlClient};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::registry::Registry;

/// Registriert alle produktiven Navigations-Tools an der Registry.
///
/// Ein gemeinsamer Fetcher für alle Tools, damit sie denselben
/// Manifestations-Cache teilen (ein Erlass wird pro Fassung genau einmal
/// geholt, egal welches Tool zuerst fragt).
pub fn register_navigation_tools<C, S>(registry: &mut Registry, fetcher: Arc<AknFetcher<C, S>>)
where
    C: SparqlClient + Send + Sync + 'static,
    S: XmlSource + Send + Sync + 'static,
{
    registry.register(Arc::new(ReadArticle {
        fetcher: Arc::clone(&fetcher),
    }));
    registry.register(Arc::new(ReadElement {
        fetcher: Arc::clone(&fetcher),
    }));
    registry.register(Arc::new(GetStructure {
        fetcher: Arc::clone(&fetcher),
    }));
    registry.register(Arc::new(SearchText {
        fetcher: Arc::clone(&fetcher),
    }));
    registry.register(Arc::new(GetMetadata {
        fetcher: Arc::clone(&fetcher),
    }));
    registry.register(Arc::new(GetReferences {
        fetcher: Arc::clone(&fetcher),
    }));
    registry.register(Arc::new(GetModifications {
        fetcher: Arc::clone(&fetcher),
    }));
    registry.register(Arc::new(CompareVersions {
        fetcher: Arc::clone(&fetcher),
    }));
    registry.register(Arc::new(ReadDocument { fetcher }));
}

// ---------------------------------------------------------------------------
// Argument-Parsing & Fehler-Mapping
// ---------------------------------------------------------------------------

/// Pflicht-Argument `eli` lesen und validieren.
fn arg_eli(args: &Value) -> Result<Eli, ToolError> {
    let raw = args
        .get("eli")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidArguments("`eli` (string) fehlt".into()))?;
    Eli::new(raw).map_err(|e| ToolError::InvalidArguments(e.to_string()))
}

/// Pflicht-Argument mit gegebenem Namen als String lesen.
fn arg_str<'a>(args: &'a Value, name: &str) -> Result<&'a str, ToolError> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidArguments(format!("`{name}` (string) fehlt")))
}

/// Optionales Argument `lang` lesen (Default Deutsch).
fn arg_lang(args: &Value) -> Result<Language, ToolError> {
    match args.get("lang").and_then(Value::as_str) {
        None => Ok(Language::De),
        Some("de") => Ok(Language::De),
        Some("fr") => Ok(Language::Fr),
        Some("it") => Ok(Language::It),
        Some("en") => Ok(Language::En),
        Some("rm") | Some("roh") => Ok(Language::Roh),
        Some(other) => Err(ToolError::InvalidArguments(format!(
            "`lang` muss de|fr|it|en|rm sein, nicht `{other}`"
        ))),
    }
}

/// Bridge-Fehler in lenkende Tool-Fehler übersetzen.
fn map_bridge(err: BridgeError) -> ToolError {
    match err {
        BridgeError::Jolux(JoluxError::NotFound(what)) => ToolError::NotFound(what),
        other => ToolError::Upstream(other.to_string()),
    }
}

/// AKN-Primitiv-Fehler in lenkende Tool-Fehler übersetzen.
fn map_akn(err: fedlex_akn::AknError) -> ToolError {
    use fedlex_akn::AknError;
    match err {
        AknError::EidNotFound(eid) => ToolError::NotFound(format!("eId `{eid}`")),
        AknError::WrongElementKind { .. } | AknError::ComponentNotFound { .. } => {
            ToolError::InvalidArguments(err.to_string())
        }
        other => ToolError::Upstream(other.to_string()),
    }
}

/// Serialisierung eines bereits validierten Domänen-Typs.
fn to_value<T: serde::Serialize>(data: T) -> Result<Value, ToolError> {
    serde_json::to_value(data).map_err(|e| ToolError::Upstream(format!("serialize: {e}")))
}

/// Gemeinsamer erster Schritt aller Tools. Erlass zum Stichtag beschaffen.
async fn fetch<C, S>(
    fetcher: &AknFetcher<C, S>,
    ctx: &ToolContext,
    args: &Value,
) -> Result<Response<Arc<AknDocument>>, ToolError>
where
    C: SparqlClient + Send + Sync,
    S: XmlSource + Send + Sync,
{
    let eli = arg_eli(args)?;
    let lang = arg_lang(args)?;
    fetcher
        .fetch_akn_document(&eli, ctx.stamp.valid_as_of(), lang)
        .await
        .map_err(map_bridge)
}

// ---------------------------------------------------------------------------
// Die Tools
// ---------------------------------------------------------------------------

/// AKN-TXT-01. Volltext eines Artikels (erzwingt `<article>`).
struct ReadArticle<C, S> {
    fetcher: Arc<AknFetcher<C, S>>,
}

#[async_trait]
impl<C, S> McpTool for ReadArticle<C, S>
where
    C: SparqlClient + Send + Sync,
    S: XmlSource + Send + Sync,
{
    fn name(&self) -> &str {
        "read_article"
    }
    fn pool(&self) -> ToolPool {
        ToolPool::LocalNavigation
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Volltext eines Artikels eines Erlasses zum Stichtag (AKN-TXT-01).",
            "properties": {
                "eli": { "type": "string", "description": "Work-ELI, z.B. eli/cc/2017/762" },
                "eid": { "type": "string", "description": "Artikel-eId, z.B. art_1" },
                "lang": { "type": "string", "enum": ["de", "fr", "it", "en", "rm"], "default": "de" }
            },
            "required": ["eli", "eid"]
        })
    }
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Response<Value>, ToolError> {
        let doc = fetch(&self.fetcher, ctx, &args).await?;
        let eid = arg_str(&args, "eid")?;
        let (text, prov) = get_article_text(doc.data(), eid, ctx.stamp.valid_as_of())
            .map_err(map_akn)?
            .into_parts();
        Ok(Response::new(to_value(text)?, prov))
    }
}

/// AKN-TXT-02. Volltext eines beliebigen eId-Elements (level, chapter, ...).
struct ReadElement<C, S> {
    fetcher: Arc<AknFetcher<C, S>>,
}

#[async_trait]
impl<C, S> McpTool for ReadElement<C, S>
where
    C: SparqlClient + Send + Sync,
    S: XmlSource + Send + Sync,
{
    fn name(&self) -> &str {
        "read_element"
    }
    fn pool(&self) -> ToolPool {
        ToolPool::LocalNavigation
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Volltext eines beliebigen Gliederungselements (AKN-TXT-02). Für LEVEL_BASED-Erlasse ohne Artikel.",
            "properties": {
                "eli": { "type": "string" },
                "eid": { "type": "string", "description": "eId des Elements, z.B. lvl_1 oder chap_2" },
                "lang": { "type": "string", "enum": ["de", "fr", "it", "en", "rm"], "default": "de" }
            },
            "required": ["eli", "eid"]
        })
    }
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Response<Value>, ToolError> {
        let doc = fetch(&self.fetcher, ctx, &args).await?;
        let eid = arg_str(&args, "eid")?;
        let (text, prov) = get_element_text(doc.data(), eid, ctx.stamp.valid_as_of())
            .map_err(map_akn)?
            .into_parts();
        Ok(Response::new(to_value(text)?, prov))
    }
}

/// AKN-STR-01. Gliederung des Erlasses, optional auf einen Element-Typ geflacht.
struct GetStructure<C, S> {
    fetcher: Arc<AknFetcher<C, S>>,
}

#[async_trait]
impl<C, S> McpTool for GetStructure<C, S>
where
    C: SparqlClient + Send + Sync,
    S: XmlSource + Send + Sync,
{
    fn name(&self) -> &str {
        "get_structure"
    }
    fn pool(&self) -> ToolPool {
        ToolPool::LocalNavigation
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Inhaltsverzeichnis des Erlasses (AKN-STR-01). type_filter=article liefert die Artikel-Liste.",
            "properties": {
                "eli": { "type": "string" },
                "type_filter": { "type": "string", "description": "Optional, z.B. article oder chapter" },
                "lang": { "type": "string", "enum": ["de", "fr", "it", "en", "rm"], "default": "de" }
            },
            "required": ["eli"]
        })
    }
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Response<Value>, ToolError> {
        let doc = fetch(&self.fetcher, ctx, &args).await?;
        let filter = args.get("type_filter").and_then(Value::as_str);
        let (outline, prov) = get_document_structure(doc.data(), filter, ctx.stamp.valid_as_of())
            .map_err(map_akn)?
            .into_parts();
        Ok(Response::new(to_value(outline)?, prov))
    }
}

/// AKN-TXT-04. Deterministische Volltextsuche über die eId-Blätter.
struct SearchText<C, S> {
    fetcher: Arc<AknFetcher<C, S>>,
}

#[async_trait]
impl<C, S> McpTool for SearchText<C, S>
where
    C: SparqlClient + Send + Sync,
    S: XmlSource + Send + Sync,
{
    fn name(&self) -> &str {
        "search_text"
    }
    fn pool(&self) -> ToolPool {
        ToolPool::LocalNavigation
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Case-insensitive Textsuche innerhalb EINES Erlasses (AKN-TXT-04). Kein Ersatz für semantische Suche.",
            "properties": {
                "eli": { "type": "string" },
                "query": { "type": "string" },
                "max_hits": { "type": "integer", "default": 20, "maximum": 100 },
                "lang": { "type": "string", "enum": ["de", "fr", "it", "en", "rm"], "default": "de" }
            },
            "required": ["eli", "query"]
        })
    }
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Response<Value>, ToolError> {
        let resp = fetch(&self.fetcher, ctx, &args).await?;
        let query = arg_str(&args, "query")?;
        let max_hits = args
            .get("max_hits")
            .and_then(Value::as_u64)
            .unwrap_or(20)
            .min(100) as usize;
        let hits = search_text(resp.data(), query, max_hits);
        let prov = resp.provenance().clone();
        Ok(Response::new(to_value(hits)?, prov))
    }
}

/// AKN-DOC-02/03. FRBR-Selbstauskunft plus Struktur-Muster des Erlasses.
struct GetMetadata<C, S> {
    fetcher: Arc<AknFetcher<C, S>>,
}

#[async_trait]
impl<C, S> McpTool for GetMetadata<C, S>
where
    C: SparqlClient + Send + Sync,
    S: XmlSource + Send + Sync,
{
    fn name(&self) -> &str {
        "get_metadata"
    }
    fn pool(&self) -> ToolPool {
        ToolPool::LocalNavigation
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "FRBR-Metadaten und Struktur-Muster des Erlasses (AKN-DOC-02/03). Vor read_article aufrufen: 91 % der OC/FGA haben keine Artikel.",
            "properties": {
                "eli": { "type": "string" },
                "lang": { "type": "string", "enum": ["de", "fr", "it", "en", "rm"], "default": "de" }
            },
            "required": ["eli"]
        })
    }
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Response<Value>, ToolError> {
        let resp = fetch(&self.fetcher, ctx, &args).await?;
        let frbr = get_frbr_metadata(resp.data()).map_err(map_akn)?;
        let pattern = classify_pattern(resp.data());
        let prov = resp.provenance().clone();
        Ok(Response::new(
            json!({ "frbr": to_value(frbr)?, "pattern": to_value(pattern)? }),
            prov,
        ))
    }
}

/// AKN-TXT-03. Ganzer Erlass als destilliertes Markdown (Anzeige/Kontext).
struct ReadDocument<C, S> {
    fetcher: Arc<AknFetcher<C, S>>,
}

#[async_trait]
impl<C, S> McpTool for ReadDocument<C, S>
where
    C: SparqlClient + Send + Sync,
    S: XmlSource + Send + Sync,
{
    fn name(&self) -> &str {
        "read_document"
    }
    fn pool(&self) -> ToolPool {
        ToolPool::LocalNavigation
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Ganzer Erlass als lesbares Markdown (AKN-TXT-03). Für Zitate read_article/read_element nutzen.",
            "properties": {
                "eli": { "type": "string" },
                "lang": { "type": "string", "enum": ["de", "fr", "it", "en", "rm"], "default": "de" }
            },
            "required": ["eli"]
        })
    }
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Response<Value>, ToolError> {
        let doc = fetch(&self.fetcher, ctx, &args).await?;
        let (markdown, prov) = get_readable_document(doc.data(), ctx.stamp.valid_as_of())
            .map_err(map_akn)?
            .into_parts();
        Ok(Response::new(json!({ "markdown": markdown }), prov))
    }
}

/// AKN-REF-01. Alle Verweise des Erlasses (Body, Präambel, Fussnoten).
struct GetReferences<C, S> {
    fetcher: Arc<AknFetcher<C, S>>,
}

#[async_trait]
impl<C, S> McpTool for GetReferences<C, S>
where
    C: SparqlClient + Send + Sync,
    S: XmlSource + Send + Sync,
{
    fn name(&self) -> &str {
        "get_references"
    }
    fn pool(&self) -> ToolPool {
        ToolPool::LocalNavigation
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Alle <ref>-Verweise des Erlasses (AKN-REF-01). Fedlex-hrefs zeigen auf Work-Ebene, Stichtagsauflösung läuft über die Tools selbst.",
            "properties": {
                "eli": { "type": "string" },
                "lang": { "type": "string", "enum": ["de", "fr", "it", "en", "rm"], "default": "de" }
            },
            "required": ["eli"]
        })
    }
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Response<Value>, ToolError> {
        let doc = fetch(&self.fetcher, ctx, &args).await?;
        let (refs, prov) = get_all_references(doc.data(), ctx.stamp.valid_as_of())
            .map_err(map_akn)?
            .into_parts();
        Ok(Response::new(to_value(refs)?, prov))
    }
}

/// AKN-MOD-01. Änderungsblöcke eines Änderungserlasses (OC).
struct GetModifications<C, S> {
    fetcher: Arc<AknFetcher<C, S>>,
}

#[async_trait]
impl<C, S> McpTool for GetModifications<C, S>
where
    C: SparqlClient + Send + Sync,
    S: XmlSource + Send + Sync,
{
    fn name(&self) -> &str {
        "get_modifications"
    }
    fn pool(&self) -> ToolPool {
        ToolPool::LocalNavigation
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Änderungsblöcke (<mod>) eines Änderungserlasses mit neuem Wortlaut (AKN-MOD-01). Leer bei Konsolidierungen, dort sind Mods eingearbeitet.",
            "properties": {
                "eli": { "type": "string" },
                "lang": { "type": "string", "enum": ["de", "fr", "it", "en", "rm"], "default": "de" }
            },
            "required": ["eli"]
        })
    }
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Response<Value>, ToolError> {
        let doc = fetch(&self.fetcher, ctx, &args).await?;
        let (mods, prov) = get_modifications(doc.data(), ctx.stamp.valid_as_of())
            .map_err(map_akn)?
            .into_parts();
        Ok(Response::new(to_value(mods)?, prov))
    }
}

/// Versionsvergleich. Zwei Stichtagsfassungen desselben Erlasses, destilliert
/// zu hinzugefügten, entfernten und geänderten Artikeln (Pool Validation).
struct CompareVersions<C, S> {
    fetcher: Arc<AknFetcher<C, S>>,
}

/// Artikel-Texte eines Dokuments als Map `eid → Normtext`.
fn article_texts(doc: &AknDocument, as_of: ValidAsOf) -> BTreeMap<String, String> {
    let Ok(outline) = get_document_structure(doc, Some("article"), as_of) else {
        return BTreeMap::new();
    };
    outline
        .data()
        .iter()
        .filter_map(|n| n.eid.clone())
        .filter_map(|eid| {
            get_element_text(doc, &eid, as_of)
                .ok()
                .map(|r| (eid, r.into_parts().0.text))
        })
        .collect()
}

#[async_trait]
impl<C, S> McpTool for CompareVersions<C, S>
where
    C: SparqlClient + Send + Sync,
    S: XmlSource + Send + Sync,
{
    fn name(&self) -> &str {
        "compare_versions"
    }
    fn pool(&self) -> ToolPool {
        ToolPool::Validation
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Vergleicht die Stichtagsfassung (as_of der Anfrage) mit einer zweiten Fassung (compare_to). Liefert hinzugefügte, entfernte und geänderte Artikel als Markdown-Destillat.",
            "properties": {
                "eli": { "type": "string" },
                "compare_to": { "type": "string", "description": "Zweiter Stichtag, ISO YYYY-MM-DD" },
                "lang": { "type": "string", "enum": ["de", "fr", "it", "en", "rm"], "default": "de" }
            },
            "required": ["eli", "compare_to"]
        })
    }
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Response<Value>, ToolError> {
        let eli = arg_eli(&args)?;
        let lang = arg_lang(&args)?;
        let compare_raw = arg_str(&args, "compare_to")?;
        let compare_date = time::Date::parse(
            compare_raw,
            time::macros::format_description!("[year]-[month]-[day]"),
        )
        .map_err(|_| ToolError::InvalidArguments("`compare_to` muss ISO YYYY-MM-DD sein".into()))?;

        // Basis-Fassung zum Anfrage-Stichtag, Vergleichs-Fassung zu compare_to.
        // Die Provenance der Antwort gehört zur Basis-Fassung (ctx.stamp).
        let base = self
            .fetcher
            .fetch_akn_document(&eli, ctx.stamp.valid_as_of(), lang)
            .await
            .map_err(map_bridge)?;
        let other = self
            .fetcher
            .fetch_akn_document(&eli, ValidAsOf::new(compare_date), lang)
            .await
            .map_err(map_bridge)?;

        let base_arts = article_texts(base.data(), ctx.stamp.valid_as_of());
        let other_arts = article_texts(other.data(), ValidAsOf::new(compare_date));

        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut changed = Vec::new();
        for (eid, text) in &base_arts {
            match other_arts.get(eid) {
                None => added.push(eid.as_str()),
                Some(old) if old != text => changed.push(eid.as_str()),
                Some(_) => {}
            }
        }
        for eid in other_arts.keys() {
            if !base_arts.contains_key(eid) {
                removed.push(eid.as_str());
            }
        }

        let mut md = format!(
            "# Versionsvergleich {}\n\nBasis {} gegen {}\n\n",
            eli.as_str(),
            ctx.stamp.valid_as_of(),
            compare_date
        );
        let section = |md: &mut String, title: &str, eids: &[&str]| {
            md.push_str(&format!("## {} ({})\n", title, eids.len()));
            if eids.is_empty() {
                md.push_str("- keine\n");
            } else {
                for eid in eids {
                    md.push_str(&format!("- {eid}\n"));
                }
            }
            md.push('\n');
        };
        section(&mut md, "Nur in Basis-Fassung", &added);
        section(&mut md, "Geaendert", &changed);
        section(&mut md, "Nur in Vergleichs-Fassung", &removed);

        let prov = base.provenance().clone();
        Ok(Response::new(
            json!({
                "markdown": md,
                "added": added,
                "changed": changed,
                "removed": removed,
                "compare_to": compare_raw,
            }),
            prov,
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests — Mocks aus fedlex-jolux/fedlex-bridge, kein Netzwerk.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthResolver, ClaimRecord, Role, StaticAuthResolver};
    use crate::temporal::TemporalResolver;
    use crate::tool::ToolContext;
    use fedlex_bridge::MockXmlSource;
    use fedlex_core::TransactionTime;
    use fedlex_jolux::MockSparqlClient;
    use time::macros::{date, datetime};

    /// Canned-Ergebnis für JLX-TMP-02 (Variablen cons/date/url).
    const CONS_JSON: &str = r#"{
      "head": { "vars": ["cons", "date", "url"] },
      "results": { "bindings": [ {
        "cons": { "type": "uri", "value": "https://fedlex.data.admin.ch/eli/cc/2017/762/20260401" },
        "date": { "type": "literal", "value": "2026-04-01" },
        "url": { "type": "uri", "value": "https://fedlex.data.admin.ch/filestore/x/de/xml" }
      } ] }
    }"#;

    /// Minimales, aber strukturell echtes AKN-Dokument.
    const MINI_ACT: &str = r##"<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0">
      <act>
        <meta><identification source="#me">
          <FRBRWork>
            <FRBRuri value="https://fedlex.data.admin.ch/eli/cc/2017/762/20260401"/>
            <FRBRname xml:lang="de" value="Energiegesetz"/>
          </FRBRWork>
          <FRBRExpression><FRBRlanguage language="de"/></FRBRExpression>
        </identification></meta>
        <body>
          <article eId="art_1">
            <num>Art. 1</num>
            <paragraph eId="art_1/para_1"><content><p>Dieses Gesetz bezweckt eine sichere Energieversorgung.</p></content></paragraph>
          </article>
        </body>
      </act>
    </akomaNtoso>"##;

    fn registry() -> Registry {
        let fetcher = Arc::new(AknFetcher::new(
            MockSparqlClient::from_json(CONS_JSON),
            MockXmlSource::new(MINI_ACT),
            8,
        ));
        let mut r = Registry::new();
        register_navigation_tools(&mut r, fetcher);
        r
    }

    fn ctx() -> ToolContext {
        ctx_with_role(Role::Reader)
    }

    fn ctx_with_role(role: Role) -> ToolContext {
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
    async fn all_navigation_tools_are_listed_for_reader() {
        let names: Vec<String> = registry()
            .list_tools(Role::Reader)
            .into_iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        for expected in [
            "get_metadata",
            "get_modifications",
            "get_references",
            "get_structure",
            "read_article",
            "read_document",
            "read_element",
            "search_text",
        ] {
            assert!(names.contains(&expected.to_string()), "fehlt: {expected}");
        }
        // Validation-Pool bleibt dem Reader verborgen.
        assert!(!names.contains(&"compare_versions".to_string()));
    }

    #[tokio::test]
    async fn read_article_returns_text_with_structural_provenance() {
        let out = registry()
            .dispatch(
                &ctx(),
                "read_article",
                json!({ "eli": "eli/cc/2017/762", "eid": "art_1" }),
            )
            .await;
        assert!(
            out["data"]["text"]
                .as_str()
                .unwrap()
                .contains("Energieversorgung"),
            "Antwort war: {out}"
        );
        // ADR-004. Herkunft aus dem FRBR-Block des Dokuments selbst.
        assert_eq!(out["provenance"]["eli"], "eli/cc/2017/762");
        assert!(!out["provenance"]["valid_as_of"].is_null());
    }

    #[tokio::test]
    async fn get_structure_lists_articles() {
        let out = registry()
            .dispatch(
                &ctx(),
                "get_structure",
                json!({ "eli": "eli/cc/2017/762", "type_filter": "article" }),
            )
            .await;
        let articles = out["data"].as_array().expect("Array von OutlineNodes");
        assert_eq!(articles.len(), 1);
        assert_eq!(articles[0]["eid"], "art_1");
    }

    #[tokio::test]
    async fn search_text_finds_hits_and_carries_provenance() {
        let out = registry()
            .dispatch(
                &ctx(),
                "search_text",
                json!({ "eli": "eli/cc/2017/762", "query": "energieversorgung" }),
            )
            .await;
        let hits = out["data"].as_array().unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["eid"], "art_1/para_1");
        assert_eq!(out["provenance"]["eli"], "eli/cc/2017/762");
    }

    #[tokio::test]
    async fn get_metadata_reports_frbr_and_pattern() {
        let out = registry()
            .dispatch(&ctx(), "get_metadata", json!({ "eli": "eli/cc/2017/762" }))
            .await;
        assert_eq!(out["data"]["frbr"]["title"], "Energiegesetz");
        assert!(out["data"]["pattern"].is_object() || out["data"]["pattern"].is_string());
    }

    #[tokio::test]
    async fn read_document_renders_markdown() {
        let out = registry()
            .dispatch(&ctx(), "read_document", json!({ "eli": "eli/cc/2017/762" }))
            .await;
        let md = out["data"]["markdown"].as_str().unwrap();
        assert!(md.contains("# Energiegesetz"));
    }

    #[tokio::test]
    async fn unknown_eid_yields_not_found_hint() {
        let out = registry()
            .dispatch(
                &ctx(),
                "read_article",
                json!({ "eli": "eli/cc/2017/762", "eid": "art_999" }),
            )
            .await;
        assert!(out["error"].as_str().unwrap().contains("not found"));
        assert!(out["hint"].as_str().unwrap().contains("existiert nicht"));
    }

    #[tokio::test]
    async fn get_references_returns_empty_list_for_refless_act() {
        let out = registry()
            .dispatch(
                &ctx(),
                "get_references",
                json!({ "eli": "eli/cc/2017/762" }),
            )
            .await;
        assert!(out["data"].as_array().unwrap().is_empty(), "war: {out}");
        assert_eq!(out["provenance"]["eli"], "eli/cc/2017/762");
    }

    #[tokio::test]
    async fn get_modifications_is_empty_on_consolidation() {
        let out = registry()
            .dispatch(
                &ctx(),
                "get_modifications",
                json!({ "eli": "eli/cc/2017/762" }),
            )
            .await;
        assert!(out["data"].as_array().unwrap().is_empty(), "war: {out}");
    }

    #[tokio::test]
    async fn compare_versions_requires_validator_role() {
        let out = registry()
            .dispatch(
                &ctx(),
                "compare_versions",
                json!({ "eli": "eli/cc/2017/762", "compare_to": "2020-01-01" }),
            )
            .await;
        assert!(out["error"].is_string(), "Reader darf nicht: {out}");
    }

    #[tokio::test]
    async fn compare_versions_reports_identical_versions_as_unchanged() {
        // Der Mock liefert für beide Stichtage dasselbe XML, also darf der
        // Vergleich keine Abweichungen melden.
        let out = registry()
            .dispatch(
                &ctx_with_role(Role::Validator),
                "compare_versions",
                json!({ "eli": "eli/cc/2017/762", "compare_to": "2020-01-01" }),
            )
            .await;
        assert!(out["data"]["added"].as_array().unwrap().is_empty());
        assert!(out["data"]["changed"].as_array().unwrap().is_empty());
        assert!(out["data"]["removed"].as_array().unwrap().is_empty());
        assert!(out["data"]["markdown"]
            .as_str()
            .unwrap()
            .contains("Versionsvergleich"));
        assert_eq!(out["provenance"]["eli"], "eli/cc/2017/762");
    }

    #[tokio::test]
    async fn compare_versions_rejects_bad_date() {
        let out = registry()
            .dispatch(
                &ctx_with_role(Role::Validator),
                "compare_versions",
                json!({ "eli": "eli/cc/2017/762", "compare_to": "irgendwann" }),
            )
            .await;
        assert!(out["error"].as_str().unwrap().contains("invalid arguments"));
    }

    #[tokio::test]
    async fn missing_eli_is_invalid_arguments() {
        let out = registry()
            .dispatch(&ctx(), "read_article", json!({ "eid": "art_1" }))
            .await;
        assert!(out["error"].as_str().unwrap().contains("invalid arguments"));
    }

    #[tokio::test]
    async fn wrong_element_kind_guides_to_read_element() {
        // art_1/para_1 ist <paragraph>, read_article erzwingt <article>.
        let out = registry()
            .dispatch(
                &ctx(),
                "read_article",
                json!({ "eli": "eli/cc/2017/762", "eid": "art_1/para_1" }),
            )
            .await;
        assert!(out["error"]
            .as_str()
            .unwrap()
            .contains("erwartet <article>"));
    }

    #[tokio::test]
    async fn not_found_consolidation_propagates_as_not_found() {
        let fetcher = Arc::new(AknFetcher::new(
            MockSparqlClient::from_json(
                r#"{ "head": { "vars": ["cons","date","url"] }, "results": { "bindings": [] } }"#,
            ),
            MockXmlSource::new(""),
            8,
        ));
        let mut r = Registry::new();
        register_navigation_tools(&mut r, fetcher);
        let out = r
            .dispatch(
                &ctx(),
                "read_article",
                json!({ "eli": "eli/cc/1907/233", "eid": "art_1" }),
            )
            .await;
        assert!(out["error"].as_str().unwrap().contains("not found"));
    }
}
