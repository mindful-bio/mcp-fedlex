//! E2E-Konformanz-Suite des AKN-Lexikons (`docs/11_LEXICON_akn.md`).
//!
//! Läuft gegen das **echte** Fedlex (SPARQL für die Manifestation-URL,
//! dann XML-Download) und prüft jedes Primitiv gegen die im Lexikon
//! dokumentierten Erwartungswerte des Energiegesetzes (SR 730.0).
//!
//! Die Assertions sind tolerant formuliert (Bereiche statt Punktwerte),
//! weil die konsolidierte Fassung mit jedem Inkrafttreten wächst.
//!
//! Ausführen:
//! ```sh
//! cargo test -p fedlex-akn --test lexicon_conformance -- --ignored --test-threads 2
//! ```
//! Endpoint via `FEDLEX_SPARQL_ENDPOINT` übersteuerbar.

use fedlex_akn::*;
use fedlex_core::ValidAsOf;
use std::sync::OnceLock;
use time::macros::date;

const ENG_WORK: &str = "https://fedlex.data.admin.ch/eli/cc/2017/762";
/// Stromgesetz vom 29.9.2023 (AS 2024 679) — Änderungserlass mit 98 `<mod>`,
/// XML-Manifestation live verifiziert (2026-06-10). Der im Lexikon zitierte
/// `eli/oc/2020/752` hat KEIN XML — Manifestationen existieren erst ab ~2021.
const OC_WORK: &str = "https://fedlex.data.admin.ch/eli/oc/2024/679";
const STICHTAG: &str = "2026-06-01";

fn stichtag() -> ValidAsOf {
    ValidAsOf::new(date!(2026 - 06 - 01))
}

fn endpoint() -> String {
    std::env::var("FEDLEX_SPARQL_ENDPOINT")
        .unwrap_or_else(|_| "https://fedlex.data.admin.ch/sparqlendpoint".to_string())
}

fn sparql_urls(query: &str) -> Vec<String> {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(endpoint())
        .header("Accept", "application/sparql-results+json")
        .form(&[("query", query)])
        .send()
        .expect("SPARQL-Request fehlgeschlagen");
    assert!(resp.status().is_success(), "SPARQL HTTP {}", resp.status());
    let json: serde_json::Value = resp.json().expect("SPARQL-JSON unlesbar");
    json["results"]["bindings"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|r| r["url"]["value"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn fetch_xml(url: &str) -> String {
    let client = reqwest::blocking::Client::new();
    let resp = client.get(url).send().expect("XML-Download fehlgeschlagen");
    assert!(resp.status().is_success(), "XML HTTP {}", resp.status());
    resp.text().expect("XML-Body unlesbar")
}

/// EnG: konsolidierte Fassung zum Stichtag via FRBR-Kette auflösen
/// (entspricht JLX-TMP-02 + JLX-RES-04), `.xml`-URL in Rust herauspicken.
fn eng() -> &'static AknDocument {
    static DOC: OnceLock<AknDocument> = OnceLock::new();
    DOC.get_or_init(|| {
        let q = format!(
            "PREFIX jolux: <http://data.legilux.public.lu/resource/ontology/jolux#> \
             PREFIX xsd: <http://www.w3.org/2001/XMLSchema#> \
             SELECT ?url WHERE {{ \
               ?cons jolux:isMemberOf <{ENG_WORK}> ; \
                     jolux:dateApplicability ?date ; \
                     jolux:isRealizedBy ?expr . \
               ?expr jolux:language <http://publications.europa.eu/resource/authority/language/DEU> ; \
                     jolux:isEmbodiedBy ?manif . \
               ?manif jolux:isExemplifiedBy ?url . \
               FILTER(?date <= \"{STICHTAG}\"^^xsd:date) \
             }} ORDER BY DESC(?date)"
        );
        let url = sparql_urls(&q)
            .into_iter()
            .find(|u| u.ends_with(".xml"))
            .expect("keine XML-Manifestation für das EnG gefunden");
        AknDocument::parse(&fetch_xml(&url)).expect("EnG-XML muss parsen (X15.1)")
    })
}

/// Stromgesetz-OC: direkte Work→Expression→Manifestation-Kette.
fn oc() -> &'static AknDocument {
    static DOC: OnceLock<AknDocument> = OnceLock::new();
    DOC.get_or_init(|| {
        let q = format!(
            "PREFIX jolux: <http://data.legilux.public.lu/resource/ontology/jolux#> \
             SELECT ?url WHERE {{ \
               <{OC_WORK}> jolux:isRealizedBy ?expr . \
               ?expr jolux:language <http://publications.europa.eu/resource/authority/language/DEU> ; \
                     jolux:isEmbodiedBy ?manif . \
               ?manif jolux:isExemplifiedBy ?url . \
             }}"
        );
        let url = sparql_urls(&q)
            .into_iter()
            .find(|u| u.ends_with(".xml"))
            .expect("keine XML-Manifestation für oc/2024/679");
        AknDocument::parse(&fetch_xml(&url)).expect("OC-XML muss parsen")
    })
}

// ───────────────────────── DOC ─────────────────────────

#[test]
#[ignore = "E2E gegen Live-Fedlex"]
fn akn_doc_02_frbr_metadata_eng() {
    let m = get_frbr_metadata(eng()).unwrap();
    // Live-Befund (2026-06-10): Konsolidierungs-XML trägt absolute, datierte
    // Work-URIs — nicht die relativen des Analyse-Snapshots.
    assert!(
        m.eli_work.contains("/eli/cc/2017/762"),
        "Work-URI: {}",
        m.eli_work
    );
    assert_eq!(m.language.as_deref(), Some("de"));
    assert!(
        m.sr_number.as_deref().unwrap_or("").contains("730.0"),
        "SR-Nummer: {:?}",
        m.sr_number
    );
    // Live-Befund: FRBRname ist mehrsprachig — der de-Titel muss gewählt werden.
    assert!(
        m.title.as_deref().unwrap_or("").contains("Energiegesetz"),
        "Titel: {:?}",
        m.title
    );
    // X19.4: FRBRdate-Namen sind jolux-präfixiert.
    assert!(m.dates.iter().any(|(n, _)| n.starts_with("jolux:")));
    // X17.6: konsolidierte Fassungen tragen oft mehrere Expression-Blöcke.
    assert!(m.expression_count >= 1);
}

#[test]
#[ignore = "E2E gegen Live-Fedlex"]
fn akn_doc_03_pattern_eng() {
    let info = classify_pattern(eng());
    assert!(info.has_body);
    assert!(
        matches!(
            info.pattern,
            DocPattern::FlatArticles | DocPattern::Structured
        ),
        "EnG-Muster: {:?}",
        info.pattern
    );
    // Lexikon-Erwartung: ~105 Artikel (Stand Analyse-Snapshot).
    assert!(
        (80..=140).contains(&info.article_count),
        "Artikel: {}",
        info.article_count
    );
}

// ───────────────────────── STR ─────────────────────────

#[test]
#[ignore = "E2E gegen Live-Fedlex"]
fn akn_str_structure_resolve_and_path_eng() {
    let resp = get_document_structure(eng(), Some("article"), stichtag()).unwrap();
    // Provenance-ELI ist die datumslose Work-Ebene — stabil über
    // Konsolidierungen, joinbar mit JOLux (work_eli_path-Normalisierung).
    assert_eq!(resp.provenance().eli.as_str(), "eli/cc/2017/762");
    let articles = resp.data();
    let info = classify_pattern(eng());
    assert_eq!(articles.len(), info.article_count);

    let hit = resolve_eid(eng(), "art_1").unwrap();
    assert_eq!(eng().tag(hit.node), "article");
    assert_eq!(hit.duplicates, 0);

    // Pfad eines tiefen Elements: erste Artikel-eId mit Unterstruktur nehmen.
    let deep = articles
        .iter()
        .filter_map(|a| a.eid.as_deref())
        .next()
        .unwrap();
    let path = get_section_path(eng(), deep).unwrap();
    assert!(!path.is_empty());
    assert_eq!(path.last().unwrap().kind, "article");
}

// ───────────────────────── TXT ─────────────────────────

#[test]
#[ignore = "E2E gegen Live-Fedlex"]
fn akn_txt_article_text_and_notes_eng() {
    let resp = get_article_text(eng(), "art_1", stichtag()).unwrap();
    let t = resp.data();
    assert!(t.text.contains("Energie"), "Art. 1: {}", t.text);
    assert!(!t.text.is_empty());

    // Falltrap-Lock X12.4: Änderungshistorie-Notes tragen AS-/SR-refs.
    let notes = extract_change_notes(eng(), None, stichtag()).unwrap();
    let with_refs = notes.data().iter().filter(|n| !n.refs.is_empty()).count();
    assert!(
        with_refs * 2 > notes.data().len(),
        "nur {with_refs}/{} Notes mit refs",
        notes.data().len()
    );
    // Notes dürfen nicht in den Normtext lecken.
    assert!(!t.text.contains("Fassung gemäss"));
}

#[test]
#[ignore = "E2E gegen Live-Fedlex"]
fn akn_txt_search_eng() {
    let hits = search_text(eng(), "Netzzuschlag", 20);
    assert!(!hits.is_empty(), "EnG muss 'Netzzuschlag' enthalten");
    assert!(hits.iter().all(|h| !h.snippet.is_empty()));
}

#[test]
#[ignore = "E2E gegen Live-Fedlex"]
fn akn_txt_03_readable_markdown_eng() {
    let md = get_readable_document(eng(), stichtag()).unwrap();
    let md = md.data();
    assert!(md.contains("Energiegesetz"), "Titel fehlt");
    assert!(md.contains("Art. 1"));
    let chars = md.chars().count();
    // Grössenordnung X20.1: lesbarer Text liegt weit unter den 1.18 MB XML.
    assert!(
        (10_000..=400_000).contains(&chars),
        "Markdown-Länge: {chars}"
    );
}

// ───────────────────────── MOD ─────────────────────────

#[test]
#[ignore = "E2E gegen Live-Fedlex"]
fn akn_mod_01_modifications_oc_stromgesetz() {
    let resp = get_modifications(oc(), stichtag()).unwrap();
    let mods = resp.data();
    // Live verifiziert (2026-06-10): 98 <mod>-Blöcke.
    assert!((50..=150).contains(&mods.len()), "mods: {}", mods.len());
    // Falltrap-Lock X7.2: 1:1-Invariante mod↔quotedStructure.
    let with_quoted = mods.iter().filter(|m| m.quoted_root_kind.is_some()).count();
    assert!(
        with_quoted * 10 >= mods.len() * 9,
        "nur {with_quoted}/{} mods mit quotedStructure",
        mods.len()
    );
    // X7.3: zitierte Wurzel überwiegend paragraph.
    let para = mods
        .iter()
        .filter(|m| m.quoted_root_kind.as_deref() == Some("paragraph"))
        .count();
    assert!(
        para * 2 > with_quoted,
        "paragraph-Anteil: {para}/{with_quoted}"
    );
}

// ───────────────────────── REF ─────────────────────────

#[test]
#[ignore = "E2E gegen Live-Fedlex"]
fn akn_ref_01_references_eng() {
    let resp = get_all_references(eng(), stichtag()).unwrap();
    let refs = resp.data();
    assert!(refs.len() > 50, "refs: {}", refs.len());
    // X11.2: Mehrheit mit absoluter Fedlex-URL …
    let fedlex = refs
        .iter()
        .filter(|r| {
            r.href
                .as_deref()
                .is_some_and(|h| h.starts_with("https://fedlex.data.admin.ch/eli/"))
        })
        .count();
    assert!(
        fedlex * 2 > refs.len(),
        "fedlex-hrefs: {fedlex}/{}",
        refs.len()
    );
    // Live-Befund (2026-06-10): Im aktuellen EnG tragen ALLE refs ein href —
    // die 15 % href-losen (X11.2) sind eine Corpus-Quote, keine
    // Dokument-Garantie. REF-02 muss trotzdem jedes Label verkraften.
    for r in refs.iter().take(50) {
        let _ = parse_unlinked_ref(&r.label);
    }
}

// ───────────────────────── CMP/SPC ─────────────────────────

#[test]
#[ignore = "E2E gegen Live-Fedlex"]
fn akn_cmp_components_eng() {
    let comps = list_components(eng());
    // EnG hat Anhänge — falls Components vorhanden, muss ihr FRBR komplett sein.
    for c in &comps {
        if !c.is_empty_stub {
            assert!(c.eli_work.is_some(), "Component {} ohne ELI", c.index);
        }
    }
    if !comps.is_empty() {
        let inner = get_component_document(eng(), 0).unwrap();
        assert_eq!(inner.tag(inner.root()), "doc");
        let _ = classify_pattern(&inner);
    }
}

#[test]
#[ignore = "E2E gegen Live-Fedlex"]
fn akn_spc_tables_and_foreign_eng() {
    let tables = extract_tables(eng(), None).unwrap();
    for t in &tables {
        assert_eq!(t.oversized, t.rows > 100);
        assert!(t.cols > 0 || t.rows == 0);
    }
    let foreign = detect_foreign_content(eng());
    for f in &foreign {
        assert!(f.element_count > 0);
    }
}

// ───────────────────────── CHK ─────────────────────────

#[test]
#[ignore = "E2E gegen Live-Fedlex"]
fn akn_chk_01_hollowing_eng() {
    let hollowed = hollow_document(eng());
    // Lexikon-Erwartung (Snapshot): 779 eIds, 655 Blätter (84.1 %).
    assert!(
        (600..=1300).contains(&hollowed.len()),
        "eIds: {}",
        hollowed.len()
    );
    let leaves = hollowed.iter().filter(|h| h.is_leaf).count();
    let ratio = leaves as f64 / hollowed.len() as f64;
    assert!(
        (0.70..=0.95).contains(&ratio),
        "Blatt-Quote: {ratio:.3} ({leaves}/{})",
        hollowed.len()
    );
    // X20.2: Eltern-Platzhalter statt redundantem Text.
    assert!(
        hollowed
            .iter()
            .filter(|h| !h.is_leaf)
            .all(|h| h.text.starts_with("[Siehe Unterelemente:"))
    );
}

#[test]
#[ignore = "E2E gegen Live-Fedlex"]
fn akn_chk_02_chunks_eng() {
    let chunks = chunk_document(eng()).unwrap();
    let info = classify_pattern(eng());
    // Mindestens ein Chunk pro Artikel, Übergrosse zerfallen in Absätze.
    assert!(
        chunks.len() >= info.article_count,
        "chunks: {} < Artikel: {}",
        chunks.len(),
        info.article_count
    );
    assert!(chunks.len() <= info.article_count * 10);
    for c in &chunks {
        assert!(!c.text.is_empty());
        // Chunk-IDs auf datumsloser Work-Ebene — stabil über Konsolidierungen.
        assert!(c.chunk_id.starts_with("eli/cc/2017/762#"), "{}", c.chunk_id);
        let m = &c.metadata;
        assert_eq!(m.collection.as_deref(), Some("cc"));
        assert_eq!(m.eli.as_deref(), Some("eli/cc/2017/762"));
        assert!(m.sr.as_deref().unwrap_or("").contains("730.0"));
        assert!(m.eid.is_some());
    }
}
