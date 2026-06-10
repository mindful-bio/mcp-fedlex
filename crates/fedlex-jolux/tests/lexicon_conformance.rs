//! # Lexikon-Konformanz-Suite (Test-Ort)
//!
//! Prüft **jeden Eintrag** aus `docs/10_LEXICON_jolux.md` live gegen den
//! Fedlex-SPARQL-Endpoint. Ein Test pro Lexikon-ID (`JLX-<DOM>-<NN>`), plus
//! Audit-Tests für die explizit ausgeschlossenen Phantom-Prädikate.
//!
//! Zwei Prüfebenen pro Eintrag:
//! 1. **Capability:** das Rust-Primitiv (Ende-zu-Ende) bzw. sein SPARQL-Muster
//!    liefert live Daten.
//! 2. **Erwartung:** die empirischen Behauptungen (Falltraps, Füllraten,
//!    Richtungen) aus dem Rulebook J0–J20 halten weiterhin.
//!
//! Seit 2026-06-10 sind **alle 27 Primitive in Rust implementiert** — jeder
//! Test ruft die öffentliche Funktion auf, nicht nur das Query-Muster.
//!
//! ## Ausführen
//!
//! Alle Tests sind `#[ignore]`, damit `cargo test` offline bleibt. Live:
//!
//! ```sh
//! cargo test -p fedlex-jolux --test lexicon_conformance -- --ignored --test-threads 2
//! ```
//!
//! `--test-threads 2` schont den öffentlichen Endpoint (Rulebook J17.5).
//! Endpoint überschreibbar via `FEDLEX_SPARQL_ENDPOINT`.

use async_trait::async_trait;
use fedlex_jolux::{
    check_in_force, explore_node, find_related_by_topic, find_treaties, get_article_history,
    get_citations, get_consultation_documents, get_consultations, get_drafts, get_fga_documents,
    get_impacts, get_law_metadata, get_memorial, get_oc_act, get_outgoing_impacts,
    get_subdivisions, get_taxonomy, get_treaty_info, list_annexes, list_expressions,
    list_versions, list_vocabulary, resolve_consolidation_at, resolve_manifestation,
    resolve_sr_number, resolve_vocabulary_label, search_law, CitationDirection, JoluxError,
    Language, ManifestationFormat, SparqlClient, SparqlResults, PREFIXES,
};
use time::macros::date;

// ============================================================
// Referenz-Erlass: EnG (SR 730.0) — Testobjekt des Rulebooks
// ============================================================
const ENG_CA: &str = "https://fedlex.data.admin.ch/eli/cc/2017/762";
const ENG_OC: &str = "https://fedlex.data.admin.ch/eli/oc/2017/762";
const ENG_FGA_TEXT: &str = "https://fedlex.data.admin.ch/eli/fga/2016/1642";
const VOCAB_BASE: &str = "https://fedlex.data.admin.ch/vocabulary/";

// ============================================================
// Live-Client (nur für diese Suite; Produktion nutzt lod_gateway)
// ============================================================
struct LiveClient {
    http: reqwest::Client,
    endpoint: String,
}

impl LiveClient {
    fn new() -> Self {
        let endpoint = std::env::var("FEDLEX_SPARQL_ENDPOINT")
            .unwrap_or_else(|_| "https://fedlex.data.admin.ch/sparqlendpoint".to_string());
        let http = reqwest::Client::builder()
            .user_agent("mcp-fedlex-lexicon-conformance/0.1 (mindful.bio)")
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("reqwest client");
        Self { http, endpoint }
    }
}

#[async_trait]
impl SparqlClient for LiveClient {
    async fn query(&self, sparql: &str) -> Result<SparqlResults, JoluxError> {
        let resp = self
            .http
            .post(&self.endpoint)
            .header("Accept", "application/sparql-results+json")
            .form(&[("query", sparql)])
            .send()
            .await
            .map_err(|e| JoluxError::Transport(e.to_string()))?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| JoluxError::Transport(e.to_string()))?;
        if !status.is_success() {
            return Err(JoluxError::Transport(format!("HTTP {status}: {body}")));
        }
        SparqlResults::from_json(&body)
    }
}

/// Roh-Query mit Pflicht-Prefixes (für Falltrap-/Phantom-Wachen neben den E2E-Aufrufen).
async fn q(client: &LiveClient, body: &str) -> SparqlResults {
    client
        .query(&format!("{PREFIXES}{body}"))
        .await
        .expect("live query failed")
}

/// Referenz-ELI (EnG) als validierter Typ.
fn eng() -> fedlex_core::Eli {
    fedlex_core::Eli::new("eli/cc/2017/762").unwrap()
}

/// Fester Stichtag der Suite (deterministische Provenance).
fn stichtag() -> fedlex_core::ValidAsOf {
    fedlex_core::ValidAsOf::new(date!(2026 - 06 - 01))
}

// ============================================================
// Domäne 1 — Identität & Auflösung (RES)
// ============================================================

/// JLX-RES-01 · resolve_sr_number — Rust-Primitiv live. SR-Nummern werden
/// **wiederverwendet** (Live-Befund 2026-06-10): 730.0 zeigt auf das alte EnG
/// (eli/cc/1999/27, aufgehoben) UND das neue (eli/cc/2017/762) — deshalb Liste
/// mit Geltungs-Status zur Disambiguierung.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_res_01_resolve_sr_number() {
    let c = LiveClient::new();
    let hits = resolve_sr_number(&c, "730.0", Language::De)
        .await
        .expect("resolve_sr_number live");
    assert!(
        hits.iter().any(|h| h.eli == "eli/cc/2017/762"),
        "SR 730.0 muss (auch) auf das aktuelle EnG zeigen, got: {hits:?}"
    );
    assert!(
        hits.len() >= 2,
        "SR-Wiederverwendung: 730.0 muss >= 2 CAs treffen (alt 1999/27 + neu 2017/762)"
    );
    let current = hits.iter().find(|h| h.eli == "eli/cc/2017/762").unwrap();
    assert!(
        current.in_force_status.as_deref().unwrap_or("").ends_with("/0"),
        "Disambiguierung: aktuelles EnG muss enforcement-status/0 tragen"
    );
    assert!(
        current.title.as_deref().unwrap_or("").contains("Energie"),
        "Titel der CA-direkten Expression erwartet, got: {:?}",
        current.title
    );
}

/// JLX-RES-02 · search_law — Rust-Primitiv live: Titelsuche findet das EnG.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_res_02_search_law() {
    let c = LiveClient::new();
    let hits = search_law(&c, "Energiegesetz", Language::De, 20)
        .await
        .expect("search_law live");
    assert!(
        hits.iter().any(|h| h.sr_number.as_deref() == Some("730.0")),
        "Suche 'Energiegesetz' muss SR 730.0 enthalten, got: {hits:?}"
    );
}

/// JLX-RES-03 · get_law_metadata — Rust-Primitiv live + Falltrap:
/// Genre/Zuständigkeit sind auf CA-Ebene leer (Rulebook J1.2, 0/69'350).
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_res_03_get_law_metadata() {
    let c = LiveClient::new();
    let eli = fedlex_core::Eli::new("eli/cc/2017/762").unwrap();
    let resp = get_law_metadata(&c, &eli, fedlex_core::ValidAsOf::new(date!(2026 - 01 - 01)))
        .await
        .expect("get_law_metadata live");
    let m = resp.data();
    assert_eq!(m.sr_number.as_deref(), Some("730.0"));
    assert!(
        m.title.as_deref().unwrap_or("").contains("Energie"),
        "Titel muss 'Energie' enthalten: {:?}",
        m.title
    );
    assert!(m.date_entry_in_force.is_some(), "EnG hat dateEntryInForce");

    // Falltrap J1.2: legalResourceGenre + responsibilityOf auf CA leer.
    let res = q(
        &c,
        &format!(
            r#"SELECT ?g ?r WHERE {{
              {{ <{ENG_CA}> jolux:legalResourceGenre ?g }}
              UNION
              {{ <{ENG_CA}> jolux:responsibilityOf ?r }}
            }} LIMIT 1"#
        ),
    )
    .await;
    assert!(
        res.is_empty(),
        "Genre/Zuständigkeit dürfen auf CA-Ebene NICHT befüllt sein (J1.2) — \
         falls doch: Datenmodell hat sich geändert, Lexikon aktualisieren!"
    );
}

/// JLX-RES-04 · resolve_manifestation — Rust-Primitiv live: FRBR-Kette liefert XML-URL.
/// Falltrap J2.1: die Richtung `<CA> isMemberOf ?x` liefert 0 Ergebnisse.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_res_04_resolve_manifestation() {
    let c = LiveClient::new();
    let resp = resolve_manifestation(&c, &eng(), stichtag(), Language::De, ManifestationFormat::Xml)
        .await
        .expect("resolve_manifestation live");
    assert!(resp.data().url.contains("xml"), "XML-URL erwartet: {}", resp.data().url);
    assert!(!resp.data().consolidation_date.is_empty());

    // Falltrap: falsche Richtung liefert nichts.
    let wrong = q(
        &c,
        &format!(r#"SELECT ?x WHERE {{ <{ENG_CA}> jolux:isMemberOf ?x }} LIMIT 1"#),
    )
    .await;
    assert!(
        wrong.is_empty(),
        "FRBR-Falltrap J2.1: CA->isMemberOf muss leer sein (eingehende Kante!)"
    );
}

/// JLX-RES-05 · list_expressions — Rust-Primitiv live: EnG in >= 3 Amtssprachen.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_res_05_list_expressions() {
    let c = LiveClient::new();
    let langs = list_expressions(&c, &eng()).await.expect("list_expressions live");
    assert!(
        langs.len() >= 3,
        "EnG muss in >= 3 Sprachen vorliegen (J13.1), got {langs:?}"
    );
    assert!(
        langs.iter().any(|l| l.ends_with("/DEU")),
        "EU-Sprachvokabular-URIs erwartet, got {langs:?}"
    );
}

// ============================================================
// Domäne 2 — Temporalität & Versionen (TMP)
// ============================================================

/// JLX-TMP-01 · list_versions — Rust-Primitiv live: EnG hat >= 13 Fassungen (J14.1).
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_tmp_01_list_versions() {
    let c = LiveClient::new();
    let resp = list_versions(&c, &eng(), stichtag()).await.expect("list_versions live");
    let versions = resp.data();
    assert!(
        versions.len() >= 13,
        "EnG hatte 2026-04 bereits 13 Fassungen (J14.1), got {}",
        versions.len()
    );
    // Chronologie eingelockt.
    assert!(
        versions.windows(2).all(|w| w[0].date_applicability <= w[1].date_applicability),
        "Fassungen müssen chronologisch sortiert sein"
    );
}

/// JLX-TMP-02 · resolve_version_at — Rust-Primitiv live: Stichtagsauflösung.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_tmp_02_resolve_version_at() {
    let c = LiveClient::new();
    let eli = fedlex_core::Eli::new("eli/cc/2017/762").unwrap();
    let resp = resolve_consolidation_at(
        &c,
        &eli,
        fedlex_core::ValidAsOf::new(date!(2023 - 06 - 15)),
        Language::De,
    )
    .await
    .expect("resolve_consolidation_at live");
    let cons = resp.data();
    assert!(
        cons.date_applicability.as_str() <= "2023-06-15",
        "Fassung darf nicht nach dem Stichtag liegen: {}",
        cons.date_applicability
    );
    assert!(cons.xml_url.contains("xml"), "XML-URL erwartet");
}

/// JLX-TMP-03 · check_in_force — Rust-Primitiv live: EnG in Kraft; Falltrap J3.3:
/// es existieren CAs ohne inForceStatus (15.1%) -> OPTIONAL ist Pflicht.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_tmp_03_check_in_force() {
    let c = LiveClient::new();
    let resp = check_in_force(&c, &eng(), stichtag()).await.expect("check_in_force live");
    assert!(resp.data().in_force, "EnG muss am Stichtag in Kraft sein");
    assert!(
        resp.data().status_uri.as_deref().unwrap_or("").ends_with("/0"),
        "enforcement-status/0 erwartet"
    );
    assert!(resp.data().date_entry_in_force.is_some());

    // Datumslogik-Gegenprobe: vor Inkrafttreten (2018-01-01) nicht in Kraft.
    let before = check_in_force(&c, &eng(), fedlex_core::ValidAsOf::new(date!(2017 - 01 - 01)))
        .await
        .expect("check_in_force live (before)");
    assert!(
        !before.data().in_force,
        "Doppel-Logik J3.2: vor Inkrafttreten darf das Status-Feld nicht gewinnen"
    );

    // Falltrap: CAs ohne Status existieren.
    let missing = q(
        &c,
        r#"SELECT ?ca WHERE {
          ?ca a jolux:ConsolidationAbstract .
          FILTER NOT EXISTS { ?ca jolux:inForceStatus ?s }
        } LIMIT 1"#,
    )
    .await;
    assert!(
        !missing.is_empty(),
        "J3.3: Es muss CAs ohne inForceStatus geben — sonst Lexikon-Falltrap obsolet"
    );
}

// ============================================================
// Domäne 3 — Struktur-Referenzen (SUB)
// ============================================================

/// JLX-SUB-01 · get_subdivisions — Rust-Primitiv live; Lückenkatalog-Beweis:
/// deutlich weniger Subdivisions als die 779 XML-eIds des EnG (J4.1).
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_sub_01_get_subdivisions() {
    let c = LiveClient::new();
    let resp = get_subdivisions(&c, &eng(), stichtag(), None)
        .await
        .expect("get_subdivisions live");
    let n = resp.data().len();
    assert!(n >= 30, "EnG muss Subdivisions haben (J4.5b: ~68), got {n}");
    assert!(
        n < 500,
        "Lückenkatalog-Behauptung J4.1 verletzt: {n} Subdivisions — \
         JOLux dürfte nie die volle XML-Struktur (779 eIds) kennen"
    );
}

/// JLX-SUB-02 · list_annexes — Rust-Primitiv live. Das EnG selbst hat keine
/// Annex-Subdivisions (live verifiziert) — leere Liste ist dort korrekt.
/// Systemweit müssen Annexe existieren (J18.2b: 1'611).
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_sub_02_list_annexes() {
    let c = LiveClient::new();
    let resp = list_annexes(&c, &eng(), stichtag()).await.expect("list_annexes live");
    assert!(
        resp.data().is_empty(),
        "Live-Befund 2026-06: EnG hat keine Annex-Subdivisions — falls jetzt doch, Lexikon prüfen"
    );

    // Systemweite Existenz (Capability-Beweis der Annex-Typ-URI).
    let res = q(
        &c,
        &format!(
            r#"SELECT ?sub WHERE {{
              ?sub jolux:legalResourceSubdivisionType <{VOCAB_BASE}subdivision-type/annex> .
            }} LIMIT 5"#
        ),
    )
    .await;
    assert!(
        !res.is_empty(),
        "Annex-Subdivisions müssen existieren (J18.2b: 1'611). \
         Falls leer: Vokabular-URI für 'annex' prüfen (subdivision-type)"
    );
}

// ============================================================
// Domäne 4 — Änderungsgraph (IMP)
// ============================================================

/// JLX-IMP-01 · get_impacts — Rust-Primitiv live: EnG hat Änderungshistorie.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_imp_01_get_impacts() {
    let c = LiveClient::new();
    let resp = get_impacts(&c, &eng(), stichtag()).await.expect("get_impacts live");
    let impacts = resp.data();
    assert!(!impacts.is_empty(), "EnG muss Impacts haben");
    assert!(
        impacts.iter().any(|i| i.impact_type.is_some()),
        "Mindestens ein Impact braucht einen Typ"
    );
}

/// JLX-IMP-02 · get_article_history — Rust-Primitiv live: Impacts auf EnG-Artikel.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_imp_02_get_article_history() {
    let c = LiveClient::new();
    let resp = get_article_history(&c, &eng(), "art", stichtag())
        .await
        .expect("get_article_history live");
    assert!(
        !resp.data().is_empty(),
        "Impacts auf einzelne EnG-Artikel müssen existieren (J6.1)"
    );
}

/// JLX-IMP-03 · get_outgoing_impacts — Rust-Primitiv live: der EnG-Erlass (OC)
/// änderte andere Gesetze (Energiestrategie 2050).
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_imp_03_get_outgoing_impacts() {
    let c = LiveClient::new();
    let oc = fedlex_core::Eli::new("eli/oc/2017/762").unwrap();
    let resp = get_outgoing_impacts(&c, &oc, stichtag())
        .await
        .expect("get_outgoing_impacts live");
    assert!(
        !resp.data().is_empty(),
        "Der EnG-Erlass (Energiestrategie 2050) muss ausgehende Impacts haben"
    );
    assert!(
        resp.data().iter().any(|i| !i.target.contains("/eli/oc/2017/762")),
        "Ausgehende Impacts müssen fremde Gesetze treffen"
    );
}

// ============================================================
// Domäne 5 — Zitationsgraph (CIT)
// ============================================================

/// JLX-CIT-01 · get_citations — Rust-Primitiv live: EnG hat formale Zitationen
/// (J7.3: ~46). Live-Befund (2026-06-10): `descriptionFrom` ist **nicht mehr
/// leer** (Rulebook J7.2 überholt) — der Test lockt den neuen Zustand ein.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_cit_01_get_citations() {
    let c = LiveClient::new();
    let resp = get_citations(&c, &eng(), CitationDirection::Both, stichtag())
        .await
        .expect("get_citations live");
    let citations = resp.data();
    assert!(!citations.is_empty(), "EnG muss Zitationen haben (J7.3)");
    assert!(
        citations.iter().any(|cit| cit.description.is_some()),
        "Live-Befund 2026-06: descriptionFrom ist beim EnG befüllt. \
         Falls wieder leer: Lexikon-Eintrag JLX-CIT-01 erneut anpassen"
    );
    // Dedup-Garantie (J7.4) eingelockt.
    let mut pairs: Vec<(&str, &str)> = citations.iter().map(|x| (x.from.as_str(), x.to.as_str())).collect();
    pairs.sort_unstable();
    let before = pairs.len();
    pairs.dedup();
    assert_eq!(before, pairs.len(), "get_citations darf keine (from,to)-Duplikate liefern");
}

// ============================================================
// Domäne 6 — Thematische Navigation (TAX)
// ============================================================

/// JLX-TAX-01 · get_taxonomy — Rust-Primitiv live: EnG klassifiziert, Hierarchie via skos:broader.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_tax_01_get_taxonomy() {
    let c = LiveClient::new();
    let resp = get_taxonomy(&c, &eng(), stichtag(), Language::De)
        .await
        .expect("get_taxonomy live");
    let entries = resp.data();
    assert!(!entries.is_empty(), "EnG muss Taxonomie-Einträge haben (J20.3)");
    assert!(
        entries.iter().any(|e| e.broader.is_some()),
        "Taxonomie muss hierarchisch sein (skos:broader, J20.3)"
    );
}

/// JLX-TAX-02 · find_related_by_topic — Rust-Primitiv live: thematische Nachbarn.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_tax_02_find_related_by_topic() {
    let c = LiveClient::new();
    let related = find_related_by_topic(&c, &eng(), 10)
        .await
        .expect("find_related_by_topic live");
    assert!(
        !related.is_empty(),
        "Thematische Nachbarn des EnG müssen über skos:broader findbar sein"
    );
    assert!(
        related.iter().all(|r| r.eli != "eli/cc/2017/762"),
        "Selbst-Referenz muss ausgefiltert sein"
    );
}

// ============================================================
// Domäne 7 — Publikations-Lebenszyklus (PUB)
// ============================================================

/// JLX-PUB-01 · get_oc_act — Rust-Primitiv live: basicAct zeigt auf den OC;
/// Genre dort befüllt (J8.3) — inverse Falltrap zu RES-03.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_pub_01_get_oc_act() {
    let c = LiveClient::new();
    let resp = get_oc_act(&c, &eng(), stichtag()).await.expect("get_oc_act live");
    let act = resp.data();
    assert_eq!(act.oc_uri, ENG_OC, "CC-URI <-> OC-URI deterministisch (J19.1)");
    assert!(
        act.genre.is_some(),
        "Genre muss auf OC-Act-Ebene befüllt sein (J8.3: 99.6%)"
    );
    assert!(
        act.memorial.as_deref().unwrap_or("").contains("eli/collection/"),
        "Memorial-Verweis erwartet, got: {:?}",
        act.memorial
    );
}

/// JLX-PUB-02 · get_memorial — Rust-Primitiv live: OC-Act hängt in einem
/// Wochen-Bulletin (J19.3) mit weiteren Akten.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_pub_02_get_memorial() {
    let c = LiveClient::new();
    let oc = fedlex_core::Eli::new("eli/oc/2017/762").unwrap();
    let info = get_memorial(&c, &oc, 100).await.expect("get_memorial live");
    assert!(
        info.uri.contains("eli/collection/"),
        "Memorial-URI-Muster eli/collection/.. erwartet (J19.3), got: {}",
        info.uri
    );
    assert!(
        info.acts.iter().any(|a| a.contains("eli/oc/2017/762")),
        "Der OC-Act selbst muss im Bulletin gelistet sein"
    );
}

/// JLX-PUB-03 · get_fga_documents — Rust-Primitiv live: Botschaft des EnG via
/// Draft-Komposition; Falltrap J9.3: FGA hat keinen inForceStatus.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_pub_03_get_fga_documents() {
    let c = LiveClient::new();
    let resp = get_fga_documents(&c, &eng(), stichtag())
        .await
        .expect("get_fga_documents live");
    assert!(
        resp.data().iter().any(|d| d.uri.contains("eli/fga/2016/1642")),
        "EnG-Botschaft (eli/fga/2016/1642) muss über den Draft findbar sein, got: {:?}",
        resp.data()
    );

    // Falltrap J9.3: FGA hat nie einen inForceStatus.
    let status = q(
        &c,
        &format!(r#"SELECT ?s WHERE {{ <{ENG_FGA_TEXT}> jolux:inForceStatus ?s }} LIMIT 1"#),
    )
    .await;
    assert!(
        status.is_empty(),
        "FGA darf KEINEN inForceStatus haben (J9.3)"
    );
}

// ============================================================
// Domäne 8 — Entstehungsgeschichte (GEN)
// ============================================================

/// JLX-GEN-01 · get_drafts — Rust-Primitiv live: der EnG-Draft
/// (eli/proj/2012/1295) trägt die Curia-Vista-Nummer "13.074" (J11.2).
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_gen_01_get_drafts() {
    let c = LiveClient::new();
    let resp = get_drafts(&c, &eng(), stichtag()).await.expect("get_drafts live");
    let drafts = resp.data();
    assert!(!drafts.is_empty(), "EnG muss einen Draft haben");
    let d = drafts
        .iter()
        .find(|d| d.uri.contains("eli/proj/2012/1295"))
        .expect("EnG-Draft eli/proj/2012/1295 erwartet");
    assert_eq!(
        d.parliament_draft_id.as_deref(),
        Some("13.074"),
        "Curia-Vista-Schlüssel (J11.2)"
    );
    assert!(
        d.resulting_resources.iter().any(|r| r.contains("eli/fga/2016/1642")),
        "Draft muss auch die Botschaft als Resultat tragen"
    );
}

/// JLX-GEN-02 · get_consultations — Rust-Primitiv live an einem Referenz-Draft
/// (eli/dl/proj/2021/100); Falltrap J10.2: Daten hängen am Task-Zwischenknoten.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_gen_02_get_consultations() {
    let c = LiveClient::new();
    let cons = get_consultations(&c, "https://fedlex.data.admin.ch/eli/dl/proj/2021/100")
        .await
        .expect("get_consultations live");
    assert!(!cons.is_empty(), "Referenz-Draft 2021/100 muss eine Vernehmlassung haben");
    assert!(
        cons[0].uri.contains("eli/dl/proj/"),
        "Consultation-URI-Muster eli/dl/proj/.. erwartet (J20.4)"
    );
    assert!(
        cons.iter().any(|x| x.start_date.is_some() || x.end_date.is_some()),
        "Task-Traversal (J10.2) muss Termine liefern, got: {cons:?}"
    );

    // Falltrap-Wache: ConsultationTasks existieren als Zwischenknoten.
    let task = q(&c, r#"SELECT ?t WHERE { ?t a jolux:ConsultationTask } LIMIT 1"#).await;
    assert!(!task.is_empty(), "ConsultationTasks müssen existieren (J10.2)");
}

/// JLX-GEN-03 · get_consultation_documents — Rust-Primitiv live: Dokumente der
/// Referenz-Vernehmlassung; Klassen-Existenz als Capability-Wache.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_gen_03_get_consultation_documents() {
    let c = LiveClient::new();
    let docs = get_consultation_documents(
        &c,
        "https://fedlex.data.admin.ch/eli/dl/proj/2021/100/cons_1",
    )
    .await
    .expect("get_consultation_documents live");
    // Referenz-Vernehmlassung kann leer sein — dann muss zumindest die
    // Klassen-Capability systemweit existieren.
    if docs.is_empty() {
        let pos = q(
            &c,
            r#"SELECT ?p WHERE { ?p a jolux:PositionStatementPublication } LIMIT 1"#,
        )
        .await;
        assert!(!pos.is_empty(), "PositionStatementPublications müssen existieren (J10.2: 873)");
    }

    let result = q(
        &c,
        r#"SELECT ?r WHERE { ?r a jolux:ResultOfAConsultationPublication } LIMIT 1"#,
    )
    .await;
    assert!(!result.is_empty(), "Ergebnisberichte müssen existieren (J10.2: 1'789)");
}

// ============================================================
// Domäne 9 — Völkerrecht (TRT)
// ============================================================

/// JLX-TRT-01 · get_treaty_info — Rust-Primitiv live an einem Referenz-Vertrag
/// (eli/treaty/1852/0001, live verifiziert 2026-06-10).
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_trt_01_get_treaty_info() {
    let c = LiveClient::new();
    let info = get_treaty_info(&c, "https://fedlex.data.admin.ch/eli/treaty/1852/0001")
        .await
        .expect("get_treaty_info live");
    assert!(
        !info.party_countries.is_empty(),
        "Vertrag muss Vertragsparteien haben (J12.1)"
    );
    assert!(
        info.party_countries.iter().all(|p| p.starts_with("http")),
        "treatyPartyCountry muss Vokabular-URI sein"
    );
    assert!(info.signature_date.is_some(), "Unterzeichnungsdatum erwartet");
}

/// JLX-TRT-02 · find_treaties — Rust-Primitiv live: enumerate über Bilateral-Flag.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_trt_02_find_treaties() {
    let c = LiveClient::new();
    let hits = find_treaties(&c, None, Some(true), 5).await.expect("find_treaties live");
    assert!(!hits.is_empty(), "Bilateral-Flag muss abfragbar sein (J12.2)");
    assert!(
        hits.iter().all(|h| h.process_uri.contains("eli/treaty/")),
        "TreatyProcess-URI-Muster erwartet, got: {hits:?}"
    );
}

// ============================================================
// Domäne 10 — Vokabulare & Schema (VOC)
// ============================================================

/// JLX-VOC-01 · resolve_vocabulary_term — Rust-Primitiv live:
/// resource-type/21 = Bundesgesetz, enforcement-status/0 = in Kraft.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_voc_01_resolve_vocabulary_term() {
    let c = LiveClient::new();
    let label = resolve_vocabulary_label(
        &c,
        &format!("{VOCAB_BASE}resource-type/21"),
        Language::De,
    )
    .await
    .expect("resource-type/21 muss DE-Label haben (J5.4)");
    assert!(
        label.to_lowercase().contains("bundesgesetz"),
        "resource-type/21 = Bundesgesetz (J5.5), got: {label}"
    );

    let status = resolve_vocabulary_label(
        &c,
        &format!("{VOCAB_BASE}enforcement-status/0"),
        Language::De,
    )
    .await
    .expect("enforcement-status/0 muss DE-Label haben");
    assert!(!status.is_empty());
}

/// JLX-VOC-02 · list_vocabulary — Rust-Primitiv live: resource-type hat >= 20 Konzepte.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_voc_02_list_vocabulary() {
    let c = LiveClient::new();
    let concepts = list_vocabulary(&c, "resource-type", Language::De, 60)
        .await
        .expect("list_vocabulary live");
    assert!(
        concepts.len() >= 20,
        "resource-type muss >= 20 Konzepte haben (J5.5: 23 genutzt), got {}",
        concepts.len()
    );
    assert!(
        concepts.iter().filter(|x| x.label.is_some()).count() >= 15,
        "Mehrheit der resource-type-Konzepte muss DE-Labels tragen"
    );
}

/// JLX-VOC-03 · explore_node — Rust-Primitiv live: Triple-Browsing um das EnG.
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn jlx_voc_03_explore_node() {
    let c = LiveClient::new();
    let hood = explore_node(&c, ENG_CA, 50).await.expect("explore_node live");
    assert!(
        hood.outgoing.len() >= 5,
        "EnG-Knoten muss >= 5 ausgehende Prädikate haben (J0.3: ~12), got {}",
        hood.outgoing.len()
    );
    assert!(
        !hood.incoming.is_empty(),
        "EnG muss eingehende Kanten haben (J0.3: ~86)"
    );
}

// ============================================================
// Audit — explizit ausgeschlossene Prädikate (Phantom-Wache)
// ============================================================

/// Ausschlussliste · legalResourceLegalBasis ist ein Phantom (J20.1).
/// Schlägt dieser Test fehl, hat Fedlex das Prädikat zu befüllen begonnen ->
/// neues Primitiv ins Lexikon aufnehmen!
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn audit_phantom_legal_resource_legal_basis() {
    let c = LiveClient::new();
    let res = q(
        &c,
        r#"SELECT ?s WHERE { ?s jolux:legalResourceLegalBasis ?o } LIMIT 1"#,
    )
    .await;
    assert!(
        res.is_empty(),
        "J20.1 obsolet: legalResourceLegalBasis ist jetzt befüllt — Lexikon erweitern!"
    );
}

/// Ausschlussliste · dateApplicability auf CA-Ebene bleibt ein Phantom (J3.1).
#[tokio::test]
#[ignore = "live: Netz + Fedlex-Endpoint nötig"]
async fn audit_phantom_date_applicability_on_ca() {
    let c = LiveClient::new();
    let res = q(
        &c,
        &format!(r#"SELECT ?d WHERE {{ <{ENG_CA}> jolux:dateApplicability ?d }} LIMIT 1"#),
    )
    .await;
    assert!(
        res.is_empty(),
        "J3.1 obsolet: dateApplicability nun auf CA-Ebene — Lexikon prüfen!"
    );
}
