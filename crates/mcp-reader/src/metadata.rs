//! JOLux-Metadaten-Tools des MCP-Readers (Pool::JoluxMetadata, ADR-007).
//!
//! Tranche A — Temporal. Drei dünne Hüllen um die norm-tragenden Primitive aus
//! `fedlex-jolux`: `check_in_force` (JLX-TMP-03), `list_versions` (JLX-TMP-01)
//! und `resolve_consolidation_at` (JLX-TMP-02). Sie beantworten die
//! gutachtenkritischen Fragen „Gilt die Norm zum Stichtag?", „Welche Fassungen
//! gibt es?" und „Welche Fassung galt zum Stichtag (mit XML-URL)?".
//!
//! Tranche B — Beziehungen. Vier weitere Hüllen um den Impact-/Zitationsgraphen:
//! `get_impacts` (JLX-IMP-01, „Welche Änderungen wirken auf diesen Erlass?"),
//! `get_outgoing_impacts` (JLX-IMP-03, „Welche Gesetze ändert dieser
//! Änderungserlass?"), `get_article_history` (JLX-IMP-02, „Wie wurde *dieser
//! Artikel* geändert?") und `get_citations` (JLX-CIT-01, „Wer zitiert wen?",
//! ein-/ausgehend/beide). Sie beantworten die gutachtenkritischen Fragen nach
//! Wirkungsketten und Querverweisen zwischen Erlassen.
//!
//! Tranche C — Einordnung. Drei Hüllen um Taxonomie und Struktur:
//! `get_taxonomy` (JLX-TAX-01, „In welchem Rechtsgebiet steht dieser Erlass?"),
//! `get_subdivisions` (JLX-SUB-01, „Welche Untergliederungen kennt der Graph?",
//! optional auf einen Typ gefiltert) und `list_annexes` (JLX-SUB-02, Spezialfall
//! `subdivision-type/annex`). Beide Struktur-Tools sind ein **Lückenkatalog**,
//! kein Inhaltsverzeichnis (J4.1): JOLux kennt nur Elemente mit mindestens einem
//! Impact — Vollstruktur liefert ausschliesslich der AKN-Layer; eine leere Liste
//! ist also normal und **kein** Beweis für „keine Artikel/Anhänge".
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
    CitationDirection, JoluxError, Language, SparqlClient, check_in_force, get_article_history,
    get_citations, get_impacts, get_outgoing_impacts, get_subdivisions, get_taxonomy, list_annexes,
    list_versions, resolve_consolidation_at,
};

use serde_json::{Value, json};
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
    registry.register(Arc::new(ResolveConsolidationAt {
        client: Arc::clone(&client),
    }));
    // Tranche B — Beziehungen.
    registry.register(Arc::new(GetImpacts {
        client: Arc::clone(&client),
    }));
    registry.register(Arc::new(GetOutgoingImpacts {
        client: Arc::clone(&client),
    }));
    registry.register(Arc::new(GetArticleHistory {
        client: Arc::clone(&client),
    }));
    registry.register(Arc::new(GetCitations {
        client: Arc::clone(&client),
    }));
    // Tranche C — Einordnung.
    registry.register(Arc::new(GetTaxonomy {
        client: Arc::clone(&client),
    }));
    registry.register(Arc::new(GetSubdivisions {
        client: Arc::clone(&client),
    }));
    registry.register(Arc::new(ListAnnexes { client }));
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

/// Pflicht-Argument `eid` als nicht-leerer String lesen (Normalisierung
/// übernimmt das Primitiv via `normalize_eid`).
fn arg_eid(args: &Value) -> Result<String, ToolError> {
    let raw = args
        .get("eid")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidArguments("`eid` (string) fehlt".into()))?;
    if raw.trim().is_empty() {
        return Err(ToolError::InvalidArguments(
            "`eid` darf nicht leer sein".into(),
        ));
    }
    Ok(raw.to_string())
}

/// Optionales Argument `direction` für `get_citations` (Default `both`).
fn arg_direction(args: &Value) -> Result<CitationDirection, ToolError> {
    match args.get("direction").and_then(Value::as_str) {
        None | Some("both") => Ok(CitationDirection::Both),
        Some("outgoing") => Ok(CitationDirection::Outgoing),
        Some("incoming") => Ok(CitationDirection::Incoming),
        Some(other) => Err(ToolError::InvalidArguments(format!(
            "`direction` muss outgoing|incoming|both sein, nicht `{other}`"
        ))),
    }
}

/// Optionales Argument `type_uri` für `get_subdivisions` lesen. Wenn gesetzt,
/// muss es eine nicht-leere Vokabular-URI sein; das Primitiv saniert sie
/// zusätzlich gegen Injektion.
fn arg_type_uri(args: &Value) -> Result<Option<String>, ToolError> {
    match args.get("type_uri") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if !s.trim().is_empty() => Ok(Some(s.clone())),
        Some(Value::String(_)) => Err(ToolError::InvalidArguments(
            "`type_uri` darf nicht leer sein".into(),
        )),
        Some(_) => Err(ToolError::InvalidArguments(
            "`type_uri` muss ein String sein".into(),
        )),
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
// Tranche B — Beziehungen (Impacts/Zitationen).
// ---------------------------------------------------------------------------

/// JLX-IMP-01. Welche Änderungen wirken auf diesen Erlass (eingehend)?
struct GetImpacts<C> {
    client: Arc<C>,
}

#[async_trait]
impl<C> McpTool for GetImpacts<C>
where
    C: SparqlClient + Send + Sync,
{
    fn name(&self) -> &str {
        "get_impacts"
    }
    fn pool(&self) -> ToolPool {
        ToolPool::JoluxMetadata
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Listet die Aenderungen (Impacts), die auf diesen Erlass und seine Artikel wirken (JLX-IMP-01). Caveat: seit 2023 stehen betroffene Artikel oft nur im Freitext-`comment` - eine leere Liste ist KEIN Beweis fuer 'nie geaendert'. Liefert einen BELEG (kind=norm) ueber den genannten Erlass.",
            "properties": {
                "eli": { "type": "string", "description": "ELI des Erlasses, z.B. eli/cc/2017/762" }
            },
            "required": ["eli"]
        })
    }
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Response<Value>, ToolError> {
        let eli = arg_eli(&args)?;
        let resp = get_impacts(self.client.as_ref(), &eli, ctx.stamp.valid_as_of())
            .await
            .map_err(map_jolux)?;
        into_value_response(resp)
    }
}

/// JLX-IMP-03. Welche Gesetze ändert dieser Änderungserlass (ausgehend)?
struct GetOutgoingImpacts<C> {
    client: Arc<C>,
}

#[async_trait]
impl<C> McpTool for GetOutgoingImpacts<C>
where
    C: SparqlClient + Send + Sync,
{
    fn name(&self) -> &str {
        "get_outgoing_impacts"
    }
    fn pool(&self) -> ToolPool {
        ToolPool::JoluxMetadata
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Listet die Gesetze, die dieser Aenderungserlass aendert (JLX-IMP-03, Richtung umgekehrt zu get_impacts). Nur OC/FGA-Erlasse sind Impact-Quellen - als `eli` also eli/oc/... uebergeben. Mantelerlasse buendeln viele Ziele. Liefert einen BELEG (kind=norm).",
            "properties": {
                "eli": { "type": "string", "description": "ELI des Aenderungserlasses (OC/FGA), z.B. eli/oc/2016/769" }
            },
            "required": ["eli"]
        })
    }
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Response<Value>, ToolError> {
        let eli = arg_eli(&args)?;
        let resp = get_outgoing_impacts(self.client.as_ref(), &eli, ctx.stamp.valid_as_of())
            .await
            .map_err(map_jolux)?;
        into_value_response(resp)
    }
}

/// JLX-IMP-02. Wie wurde *dieser Artikel* geändert?
struct GetArticleHistory<C> {
    client: Arc<C>,
}

#[async_trait]
impl<C> McpTool for GetArticleHistory<C>
where
    C: SparqlClient + Send + Sync,
{
    fn name(&self) -> &str {
        "get_article_history"
    }
    fn pool(&self) -> ToolPool {
        ToolPool::JoluxMetadata
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Listet die Aenderungen, die auf einen EINZELNEN Artikel eines Erlasses gewirkt haben (JLX-IMP-02). Der eID wird normalisiert (z.B. art_14_a -> art_14a). Caveat (wie get_impacts): seit 2023 oft nur im Freitext-`comment` des Gesamterlass-Impacts - leere Liste ist KEIN Beweis fuer 'nie geaendert'. Liefert einen BELEG (kind=norm).",
            "properties": {
                "eli": { "type": "string", "description": "ELI des Erlasses, z.B. eli/cc/2017/762" },
                "eid": { "type": "string", "description": "eID des Artikels, z.B. art_14a oder art_2b/para_1" }
            },
            "required": ["eli", "eid"]
        })
    }
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Response<Value>, ToolError> {
        let eli = arg_eli(&args)?;
        let eid = arg_eid(&args)?;
        let resp = get_article_history(self.client.as_ref(), &eli, &eid, ctx.stamp.valid_as_of())
            .await
            .map_err(map_jolux)?;
        into_value_response(resp)
    }
}

/// JLX-CIT-01. Formale Zitationen eines Erlasses (ein-/ausgehend/beide).
struct GetCitations<C> {
    client: Arc<C>,
}

#[async_trait]
impl<C> McpTool for GetCitations<C>
where
    C: SparqlClient + Send + Sync,
{
    fn name(&self) -> &str {
        "get_citations"
    }
    fn pool(&self) -> ToolPool {
        ToolPool::JoluxMetadata
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Listet formale Zitationen eines Erlasses (JLX-CIT-01). Richtung: outgoing (was dieser Erlass zitiert), incoming (wer ihn zitiert), both (Default). NUR Gesamttext-Granularitaet, nie Artikel-Ebene; fuer vollstaendige Zitationsnetze JOLux mit AKN-Inline-Refs mergen. Liefert einen BELEG (kind=norm).",
            "properties": {
                "eli": { "type": "string", "description": "ELI des Erlasses, z.B. eli/cc/2017/762" },
                "direction": { "type": "string", "enum": ["outgoing", "incoming", "both"], "default": "both" }
            },
            "required": ["eli"]
        })
    }
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Response<Value>, ToolError> {
        let eli = arg_eli(&args)?;
        let direction = arg_direction(&args)?;
        let resp = get_citations(
            self.client.as_ref(),
            &eli,
            direction,
            ctx.stamp.valid_as_of(),
        )
        .await
        .map_err(map_jolux)?;
        into_value_response(resp)
    }
}

// ---------------------------------------------------------------------------
// Tranche C — Einordnung (Taxonomie/Struktur).
// ---------------------------------------------------------------------------

/// JLX-TAX-01. In welchem Rechtsgebiet steht dieser Erlass?
struct GetTaxonomy<C> {
    client: Arc<C>,
}

#[async_trait]
impl<C> McpTool for GetTaxonomy<C>
where
    C: SparqlClient + Send + Sync,
{
    fn name(&self) -> &str {
        "get_taxonomy"
    }
    fn pool(&self) -> ToolPool {
        ToolPool::JoluxMetadata
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Listet die Rechtstaxonomie-Eintraege, denen ein Erlass zugeordnet ist, mit Label und skos:broader-Parent fuer Hierarchie-Traversal (JLX-TAX-01). Caveat: ~10'000 CAs sind unklassifiziert - eine leere Liste ist normal. Liefert einen BELEG (kind=norm) ueber den genannten Erlass.",
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
        let resp = get_taxonomy(self.client.as_ref(), &eli, ctx.stamp.valid_as_of(), lang)
            .await
            .map_err(map_jolux)?;
        into_value_response(resp)
    }
}

/// JLX-SUB-01. Welche Untergliederungen kennt der Graph für diesen Erlass?
struct GetSubdivisions<C> {
    client: Arc<C>,
}

#[async_trait]
impl<C> McpTool for GetSubdivisions<C>
where
    C: SparqlClient + Send + Sync,
{
    fn name(&self) -> &str {
        "get_subdivisions"
    }
    fn pool(&self) -> ToolPool {
        ToolPool::JoluxMetadata
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Listet die im Graphen bekannten Untergliederungen eines Erlasses (Artikel, Kapitel, Anhang ...), transitiv (JLX-SUB-01). WICHTIG: Luckenkatalog, KEIN Inhaltsverzeichnis - JOLux kennt nur Elemente mit mind. einem Impact (0.4-8.5 % der eIDs); Vollstruktur liefert nur der AKN-Layer. Leere Liste ist normal. Optional auf einen Subdivision-Typ gefiltert. Liefert einen BELEG (kind=norm).",
            "properties": {
                "eli": { "type": "string", "description": "ELI des Erlasses, z.B. eli/cc/2017/762" },
                "type_uri": { "type": "string", "description": "Optionale Subdivision-Typ-URI zum Filtern, z.B. https://fedlex.data.admin.ch/vocabulary/subdivision-type/article" }
            },
            "required": ["eli"]
        })
    }
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Response<Value>, ToolError> {
        let eli = arg_eli(&args)?;
        let type_uri = arg_type_uri(&args)?;
        let resp = get_subdivisions(
            self.client.as_ref(),
            &eli,
            ctx.stamp.valid_as_of(),
            type_uri.as_deref(),
        )
        .await
        .map_err(map_jolux)?;
        into_value_response(resp)
    }
}

/// JLX-SUB-02. Welche Anhänge hat dieser Erlass?
struct ListAnnexes<C> {
    client: Arc<C>,
}

#[async_trait]
impl<C> McpTool for ListAnnexes<C>
where
    C: SparqlClient + Send + Sync,
{
    fn name(&self) -> &str {
        "list_annexes"
    }
    fn pool(&self) -> ToolPool {
        ToolPool::JoluxMetadata
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "description": "Listet die Anhaenge eines Erlasses (Subdivision-Typ annex), Spezialfall von get_subdivisions (JLX-SUB-02). Eigenes Tool, weil das AKN-Mapping abweicht (Annexe erscheinen im XML als <component>, nicht <attachment>). Nur ~500 CAs haben Annexe; leere Liste ist normal. Liefert einen BELEG (kind=norm).",
            "properties": {
                "eli": { "type": "string", "description": "ELI des Erlasses, z.B. eli/cc/2017/762" }
            },
            "required": ["eli"]
        })
    }
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Response<Value>, ToolError> {
        let eli = arg_eli(&args)?;
        let resp = list_annexes(self.client.as_ref(), &eli, ctx.stamp.valid_as_of())
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

    // -----------------------------------------------------------------------
    // Tranche B — Beziehungen.
    // -----------------------------------------------------------------------

    /// Canned eingehende Impacts: zwei Änderungen.
    const IMPACTS_JSON: &str = r#"{
      "head": { "vars": ["impact","type","date","comment","from"] },
      "results": { "bindings": [
        { "impact": { "type": "uri", "value": "https://fedlex.data.admin.ch/impact/1" },
          "date": { "type": "literal", "value": "2019-06-01" },
          "comment": { "type": "literal", "value": "Art. 5, 7" } },
        { "impact": { "type": "uri", "value": "https://fedlex.data.admin.ch/impact/2" },
          "date": { "type": "literal", "value": "2021-01-01" } }
      ] }
    }"#;

    /// Canned ausgehende Impacts: ein Mantelerlass ändert ein Ziel.
    const OUTGOING_JSON: &str = r#"{
      "head": { "vars": ["impact","target","type","date"] },
      "results": { "bindings": [{
        "impact": { "type": "uri", "value": "https://fedlex.data.admin.ch/impact/9" },
        "target": { "type": "uri", "value": "https://fedlex.data.admin.ch/eli/cc/2017/762" },
        "date": { "type": "literal", "value": "2018-01-01" }
      }] }
    }"#;

    /// Canned Zitationen (outgoing/incoming-Bindings).
    const CITATIONS_JSON: &str = r#"{
      "head": { "vars": ["from","to","desc"] },
      "results": { "bindings": [
        { "from": { "type": "uri", "value": "https://fedlex.data.admin.ch/eli/cc/2017/762/text" },
          "to": { "type": "uri", "value": "https://fedlex.data.admin.ch/eli/cc/1998/3033/text" },
          "desc": { "type": "literal", "value": "Art. 31" } },
        { "from": { "type": "uri", "value": "https://fedlex.data.admin.ch/eli/cc/2017/762/text" },
          "to": { "type": "uri", "value": "https://fedlex.data.admin.ch/eli/cc/1998/3033/text" } }
      ] }
    }"#;

    #[tokio::test]
    async fn tranche_b_tools_hidden_from_reader_visible_to_navigator() {
        let r = registry_with(IMPACTS_JSON);
        let reader = r.list_tools(Role::Reader);
        assert!(reader.is_empty(), "Reader darf KEIN JoluxMetadata sehen");

        let nav: Vec<String> = r
            .list_tools(Role::Navigator)
            .into_iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        for expected in [
            "get_impacts",
            "get_outgoing_impacts",
            "get_article_history",
            "get_citations",
        ] {
            assert!(
                nav.contains(&expected.to_string()),
                "Navigator fehlt: {expected}"
            );
        }
    }

    #[tokio::test]
    async fn get_impacts_returns_changes_with_norm_provenance() {
        let r = registry_with(IMPACTS_JSON);
        let out = r
            .dispatch(
                &ctx(Role::Navigator),
                "get_impacts",
                json!({ "eli": "eli/cc/2017/762" }),
            )
            .await;
        assert!(out.get("error").is_none(), "unerwarteter Fehler: {out}");
        let impacts = out["data"].as_array().expect("impacts-Array");
        assert_eq!(impacts.len(), 2);
        assert_eq!(impacts[0]["comment"], "Art. 5, 7");
        assert_eq!(out["provenance"]["kind"], "norm");
        assert_eq!(out["provenance"]["eli"], "eli/cc/2017/762");
    }

    #[tokio::test]
    async fn get_outgoing_impacts_carries_target_and_norm_provenance() {
        let r = registry_with(OUTGOING_JSON);
        let out = r
            .dispatch(
                &ctx(Role::Navigator),
                "get_outgoing_impacts",
                json!({ "eli": "eli/oc/2016/769" }),
            )
            .await;
        assert!(out.get("error").is_none(), "unerwarteter Fehler: {out}");
        let impacts = out["data"].as_array().expect("impacts-Array");
        assert_eq!(impacts.len(), 1);
        assert_eq!(
            impacts[0]["target"],
            "https://fedlex.data.admin.ch/eli/cc/2017/762"
        );
        assert_eq!(out["provenance"]["kind"], "norm");
        assert_eq!(out["provenance"]["eli"], "eli/oc/2016/769");
    }

    #[tokio::test]
    async fn get_article_history_normalizes_eid_and_carries_norm_provenance() {
        let client = Arc::new(MockSparqlClient::from_json(IMPACTS_JSON));
        let mut r = Registry::new();
        register_metadata_tools(&mut r, Arc::clone(&client));
        let out = r
            .dispatch(
                &ctx(Role::Navigator),
                "get_article_history",
                json!({ "eli": "eli/cc/2017/762", "eid": "art_14_a" }),
            )
            .await;
        assert!(out.get("error").is_none(), "unerwarteter Fehler: {out}");
        assert_eq!(out["provenance"]["kind"], "norm");
        assert_eq!(out["provenance"]["eli"], "eli/cc/2017/762");
        // J18.2: art_14_a wird zu art_14a normalisiert (im SPARQL-Query sichtbar).
        let q = client.last_query().expect("query gestellt");
        assert!(q.contains("art_14a"), "eID nicht normalisiert: {q}");
        assert!(!q.contains("art_14_a"), "roher eID im Query: {q}");
    }

    #[tokio::test]
    async fn get_article_history_missing_eid_is_invalid_arguments() {
        let r = registry_with(IMPACTS_JSON);
        let out = r
            .dispatch(
                &ctx(Role::Navigator),
                "get_article_history",
                json!({ "eli": "eli/cc/2017/762" }),
            )
            .await;
        assert!(out["error"].as_str().unwrap().contains("invalid arguments"));
    }

    #[tokio::test]
    async fn get_citations_default_direction_is_both_and_deduplicates() {
        let client = Arc::new(MockSparqlClient::from_json(CITATIONS_JSON));
        let mut r = Registry::new();
        register_metadata_tools(&mut r, Arc::clone(&client));
        let out = r
            .dispatch(
                &ctx(Role::Navigator),
                "get_citations",
                json!({ "eli": "eli/cc/2017/762" }),
            )
            .await;
        assert!(out.get("error").is_none(), "unerwarteter Fehler: {out}");
        // Default = both → UNION-Query.
        let q = client.last_query().expect("query gestellt");
        assert!(q.contains("UNION"), "Default-Richtung nicht both: {q}");
        // J7.4: Duplikat (from,to) entfernt → eine Zitation.
        let cits = out["data"].as_array().expect("citations-Array");
        assert_eq!(cits.len(), 1);
        assert_eq!(cits[0]["description"], "Art. 31");
        assert_eq!(out["provenance"]["kind"], "norm");
        assert_eq!(out["provenance"]["eli"], "eli/cc/2017/762");
    }

    #[tokio::test]
    async fn get_citations_outgoing_direction_uses_single_clause() {
        let client = Arc::new(MockSparqlClient::from_json(CITATIONS_JSON));
        let mut r = Registry::new();
        register_metadata_tools(&mut r, Arc::clone(&client));
        let _ = r
            .dispatch(
                &ctx(Role::Navigator),
                "get_citations",
                json!({ "eli": "eli/cc/2017/762", "direction": "outgoing" }),
            )
            .await;
        let q = client.last_query().expect("query gestellt");
        assert!(!q.contains("UNION"), "outgoing sollte ohne UNION sein: {q}");
    }

    #[tokio::test]
    async fn get_citations_invalid_direction_is_invalid_arguments() {
        let r = registry_with(CITATIONS_JSON);
        let out = r
            .dispatch(
                &ctx(Role::Navigator),
                "get_citations",
                json!({ "eli": "eli/cc/2017/762", "direction": "sideways" }),
            )
            .await;
        assert!(out["error"].as_str().unwrap().contains("invalid arguments"));
    }

    #[tokio::test]
    async fn reader_call_on_get_citations_is_gracefully_denied() {
        let r = registry_with(CITATIONS_JSON);
        let out = r
            .dispatch(
                &ctx(Role::Reader),
                "get_citations",
                json!({ "eli": "eli/cc/2017/762" }),
            )
            .await;
        assert!(out["error"].as_str().unwrap().contains("not permitted"));
    }

    // -----------------------------------------------------------------------
    // Tranche C — Einordnung (Taxonomie/Struktur).
    // -----------------------------------------------------------------------

    /// Canned Taxonomie: ein Eintrag mit Label und Parent.
    const TAXONOMY_JSON: &str = r#"{
      "head": { "vars": ["tax","label","parent"] },
      "results": { "bindings": [{
        "tax": { "type": "uri", "value": "https://fedlex.data.admin.ch/vocabulary/legal-taxonomy/730" },
        "label": { "type": "literal", "xml:lang": "de", "value": "Energie" },
        "parent": { "type": "uri", "value": "https://fedlex.data.admin.ch/vocabulary/legal-taxonomy/7" }
      }] }
    }"#;

    /// Canned Subdivisions: ein Artikel (mit Typ) + eine ohne Typ.
    const SUBS_JSON: &str = r#"{
      "head": { "vars": ["sub","type"] },
      "results": { "bindings": [
        { "sub": { "type": "uri", "value": "https://fedlex.data.admin.ch/eli/cc/2017/762/art_14" },
          "type": { "type": "uri", "value": "https://fedlex.data.admin.ch/vocabulary/subdivision-type/article" } },
        { "sub": { "type": "uri", "value": "https://fedlex.data.admin.ch/eli/cc/2017/762/art_19" } }
      ] }
    }"#;

    #[tokio::test]
    async fn tranche_c_tools_hidden_from_reader_visible_to_navigator() {
        let r = registry_with(TAXONOMY_JSON);
        let reader = r.list_tools(Role::Reader);
        assert!(reader.is_empty(), "Reader darf KEIN JoluxMetadata sehen");

        let nav: Vec<String> = r
            .list_tools(Role::Navigator)
            .into_iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        for expected in ["get_taxonomy", "get_subdivisions", "list_annexes"] {
            assert!(
                nav.contains(&expected.to_string()),
                "Navigator fehlt: {expected}"
            );
        }
    }

    #[tokio::test]
    async fn get_taxonomy_resolves_label_in_requested_lang_with_norm_provenance() {
        let client = Arc::new(MockSparqlClient::from_json(TAXONOMY_JSON));
        let mut r = Registry::new();
        register_metadata_tools(&mut r, Arc::clone(&client));
        let out = r
            .dispatch(
                &ctx(Role::Navigator),
                "get_taxonomy",
                json!({ "eli": "eli/cc/2017/762", "lang": "de" }),
            )
            .await;
        assert!(out.get("error").is_none(), "unerwarteter Fehler: {out}");
        let entries = out["data"].as_array().expect("taxonomy-Array");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["label"], "Energie");
        assert!(entries[0]["broader"].as_str().unwrap().ends_with("/7"));
        assert_eq!(out["provenance"]["kind"], "norm");
        assert_eq!(out["provenance"]["eli"], "eli/cc/2017/762");
        // Sprach-Tag landet im Query.
        let q = client.last_query().expect("query gestellt");
        assert!(
            q.contains(r#"LANG(?label) = "de""#),
            "lang-Filter fehlt: {q}"
        );
    }

    #[tokio::test]
    async fn get_taxonomy_invalid_lang_is_invalid_arguments() {
        let r = registry_with(TAXONOMY_JSON);
        let out = r
            .dispatch(
                &ctx(Role::Navigator),
                "get_taxonomy",
                json!({ "eli": "eli/cc/2017/762", "lang": "xx" }),
            )
            .await;
        assert!(out["error"].as_str().unwrap().contains("invalid arguments"));
    }

    #[tokio::test]
    async fn get_subdivisions_lists_transitively_with_norm_provenance() {
        let r = registry_with(SUBS_JSON);
        let out = r
            .dispatch(
                &ctx(Role::Navigator),
                "get_subdivisions",
                json!({ "eli": "eli/cc/2017/762" }),
            )
            .await;
        assert!(out.get("error").is_none(), "unerwarteter Fehler: {out}");
        let subs = out["data"].as_array().expect("subdivisions-Array");
        assert_eq!(subs.len(), 2);
        assert!(
            subs[0]["subdivision_type"]
                .as_str()
                .unwrap()
                .ends_with("/article")
        );
        assert_eq!(out["provenance"]["kind"], "norm");
        assert_eq!(out["provenance"]["eli"], "eli/cc/2017/762");
    }

    #[tokio::test]
    async fn get_subdivisions_type_uri_filters_query() {
        let client = Arc::new(MockSparqlClient::from_json(SUBS_JSON));
        let mut r = Registry::new();
        register_metadata_tools(&mut r, Arc::clone(&client));
        let _ = r
            .dispatch(
                &ctx(Role::Navigator),
                "get_subdivisions",
                json!({
                    "eli": "eli/cc/2017/762",
                    "type_uri": "https://fedlex.data.admin.ch/vocabulary/subdivision-type/article"
                }),
            )
            .await;
        let q = client.last_query().expect("query gestellt");
        assert!(
            q.contains("subdivision-type/article"),
            "Typ-Filter nicht im Query: {q}"
        );
    }

    #[tokio::test]
    async fn get_subdivisions_empty_type_uri_is_invalid_arguments() {
        let r = registry_with(SUBS_JSON);
        let out = r
            .dispatch(
                &ctx(Role::Navigator),
                "get_subdivisions",
                json!({ "eli": "eli/cc/2017/762", "type_uri": "  " }),
            )
            .await;
        assert!(out["error"].as_str().unwrap().contains("invalid arguments"));
    }

    #[tokio::test]
    async fn list_annexes_filters_on_annex_type_with_norm_provenance() {
        let client = Arc::new(MockSparqlClient::from_json(SUBS_JSON));
        let mut r = Registry::new();
        register_metadata_tools(&mut r, Arc::clone(&client));
        let out = r
            .dispatch(
                &ctx(Role::Navigator),
                "list_annexes",
                json!({ "eli": "eli/cc/2017/762" }),
            )
            .await;
        assert!(out.get("error").is_none(), "unerwarteter Fehler: {out}");
        assert_eq!(out["provenance"]["kind"], "norm");
        assert_eq!(out["provenance"]["eli"], "eli/cc/2017/762");
        // JLX-SUB-02: Filter auf subdivision-type/annex.
        let q = client.last_query().expect("query gestellt");
        assert!(
            q.contains("subdivision-type/annex"),
            "Annex-Filter fehlt: {q}"
        );
    }

    #[tokio::test]
    async fn empty_subdivisions_is_normal_not_error() {
        let empty = r#"{ "head": { "vars": ["sub"] }, "results": { "bindings": [] } }"#;
        let r = registry_with(empty);
        let out = r
            .dispatch(
                &ctx(Role::Navigator),
                "get_subdivisions",
                json!({ "eli": "eli/cc/1999/404" }),
            )
            .await;
        assert!(
            out.get("error").is_none(),
            "leere Liste ist kein Fehler: {out}"
        );
        assert!(out["data"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn reader_call_on_get_subdivisions_is_gracefully_denied() {
        let r = registry_with(SUBS_JSON);
        let out = r
            .dispatch(
                &ctx(Role::Reader),
                "get_subdivisions",
                json!({ "eli": "eli/cc/2017/762" }),
            )
            .await;
        assert!(out["error"].as_str().unwrap().contains("not permitted"));
    }
}
