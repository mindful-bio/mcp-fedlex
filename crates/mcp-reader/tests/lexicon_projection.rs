//! # Vollständigkeits-Matrix (Lexikon → Tool-Projektion)
//!
//! Offline-CI-Test, der den letzten offenen Punkt aus `45_GAP_ANALYSIS.md` (G-4)
//! und das letzte Akzeptanzkriterium aus `adr/ADR-007-…md` (Z. 108–110) schliesst:
//! **Jedes** im Lexikon (`docs/10_LEXICON_jolux.md`, `docs/11_LEXICON_akn.md`)
//! dokumentierte Primitiv ist genau einem Zustand zugeordnet —
//! **`Projected`** (als MCP-Tool registriert) **oder** **`Excluded`** (begründeter
//! Ausschluss).
//!
//! Der Test wird **rot**, sobald
//! - ein neues Lexikon-Primitiv ohne Matrix-Eintrag dazukommt (G-4-Schutz),
//! - ein Matrix-Eintrag kein Lexikon-Pendant mehr hat (veraltet),
//! - ein `Projected`-Tool nicht (mehr) registriert ist,
//! - ein registriertes Tool weder in der Matrix noch als Composite verbucht ist
//!   (verwaist),
//! - oder die dokumentierte Zähl-Invariante (24 projiziert / 23 ausgeschlossen,
//!   + 1 Composite ohne Lexikon-Primitiv = 25 MCP-Tools) bricht.

//!
//! ## Composite-Tools ohne Lexikon-Primitiv
//!
//! Das Lexikon ist laut eigenem Vorwort *„kein Tool-Katalog eines bestimmten
//! Servers, sondern das Vokabular, aus dem Konsumenten komponieren"*. Der Reader
//! darf also Primitive zu höherwertigen Tools **komponieren**, die keinen 1:1-
//! Lexikon-Eintrag haben. Aktuell genau ein Fall: `compare_versions` (Diff aus
//! `get_document_structure` + `get_element_text`, Pool `Validation`, nur für die
//! Validator-Rolle sichtbar). Solche Tools stehen in der expliziten
//! [`COMPOSITE_TOOLS`]-Allowlist; ein *neues* Composite-Tool ohne Eintrag dort
//! lässt den Test ebenfalls brechen.
//!
//! **Kein `#[ignore]`** — der Test ist rein offline (ruft kein Tool auf, nur die
//! Registrierung) und läuft in jeder `cargo test`-Runde.
//!
//! ```sh
//! cargo test -p mcp-reader --test lexicon_projection
//! ```

use std::collections::BTreeSet;
use std::sync::Arc;

use fedlex_bridge::{AknFetcher, MockXmlSource};
use fedlex_jolux::MockSparqlClient;
use mcp_reader::{
    Registry, Role, ToolPool, register_discovery_tools, register_metadata_tools,
    register_navigation_tools,
};
use regex::Regex;

// ============================================================
// Minimale Fixtures — nur zum Bau des AknFetcher/MockSparqlClient.
// Die Tools werden NIE aufgerufen; es zählt nur, welche registriert sind.
// ============================================================

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
        <paragraph eId="art_1/para_1"><content><p>Zweck.</p></content></paragraph>
      </article>
    </body>
  </act>
</akomaNtoso>"##;

/// Leeres, aber valides SPARQL-JSON-Resultat (für Metadaten-/Discovery-Mocks).
const EMPTY_JSON: &str = r#"{ "head": { "vars": [] }, "results": { "bindings": [] } }"#;

// ============================================================
// Soll-Matrix (die Single Source of Truth des Tests, §2 des Briefings).
// ============================================================

/// Zustand pro Lexikon-Primitiv.
#[derive(Debug)]
enum Projection {
    /// Über MCP erreichbar; Wert = exakter Tool-`name()`.
    Projected(&'static str),
    /// Bewusster, begründeter Ausschluss; Wert = Begründung. Die Begründung wird
    /// nicht programmatisch gelesen, ist aber bewusst inline dokumentiert (sie ist
    /// der eigentliche Wert der Matrix), daher `allow(dead_code)`.
    #[allow(dead_code)]
    Excluded(&'static str),
}
use Projection::*;

/// Die Vollständigkeits-Matrix: jede der 47 Lexikon-IDs → Zustand.
/// `Projected("<tool_name>")` führt den **Tool-Namen** (nicht den
/// Lexikon-Funktionsnamen — diese weichen in drei Fällen ab, siehe Briefing §1.2).
const MATRIX: &[(&str, Projection)] = &[
    // ----- JOLux (27): 13 projiziert / 14 ausgeschlossen -----
    ("JLX-RES-01", Projected("resolve_sr_number")),
    ("JLX-RES-02", Projected("search_law")),
    (
        "JLX-RES-03",
        Excluded("Steckbrief; vom Reader intern genutzt, kein eigenständiges Agenten-Tool"),
    ),
    (
        "JLX-RES-04",
        Excluded("resolve_manifestation — interner Bridge-Schritt (AknFetcher)"),
    ),
    (
        "JLX-RES-05",
        Excluded("list_expressions — Sprachvarianten-Auflösung, intern"),
    ),
    ("JLX-TMP-01", Projected("list_versions")),
    // Name weicht ab: Lexikon resolve_version_at → Tool resolve_consolidation_at
    ("JLX-TMP-02", Projected("resolve_consolidation_at")),
    ("JLX-TMP-03", Projected("check_in_force")),
    ("JLX-SUB-01", Projected("get_subdivisions")),
    ("JLX-SUB-02", Projected("list_annexes")),
    ("JLX-IMP-01", Projected("get_impacts")),
    ("JLX-IMP-02", Projected("get_article_history")),
    ("JLX-IMP-03", Projected("get_outgoing_impacts")),
    ("JLX-CIT-01", Projected("get_citations")),
    ("JLX-TAX-01", Projected("get_taxonomy")),
    // Name weicht ab: Lexikon find_related_by_topic → Tool find_related_topic
    ("JLX-TAX-02", Projected("find_related_topic")),
    (
        "JLX-PUB-01",
        Excluded("get_oc_act — Publikationsschicht, noch nicht projizierte Tranche"),
    ),
    ("JLX-PUB-02", Excluded("get_memorial — dito")),
    ("JLX-PUB-03", Excluded("get_fga_documents — dito")),
    (
        "JLX-GEN-01",
        Excluded("get_drafts — Entstehungsgeschichte, kein gutachtenkritischer Pfad"),
    ),
    ("JLX-GEN-02", Excluded("get_consultations — dito")),
    ("JLX-GEN-03", Excluded("get_consultation_documents — dito")),
    (
        "JLX-TRT-01",
        Excluded("get_treaty_info — Staatsverträge, eigene spätere Tranche"),
    ),
    ("JLX-TRT-02", Excluded("find_treaties — dito")),
    (
        "JLX-VOC-01",
        Excluded("resolve_vocabulary_term — kontrolliertes Vokabular, kein Agenten-Bedarf"),
    ),
    ("JLX-VOC-02", Excluded("list_vocabulary — dito")),
    ("JLX-VOC-03", Excluded("explore_node — dito")),
    // ----- AKN (20): 11 projiziert / 9 ausgeschlossen -----
    (
        "AKN-DOC-01",
        Excluded("fetch_akn_document — produktive Bridge-Komposition, interner Fetch"),
    ),
    ("AKN-DOC-02", Projected("get_metadata")),
    (
        "AKN-DOC-03",
        Excluded("classify_pattern — intern in get_metadata genutzt"),
    ),
    ("AKN-STR-01", Projected("get_structure")),
    (
        "AKN-STR-02",
        Excluded("resolve_eid — interner Lookup, kein Agenten-Tool"),
    ),
    (
        "AKN-STR-03",
        Excluded("get_section_path — interner Pfad-Helper"),
    ),
    ("AKN-TXT-01", Projected("read_article")),
    ("AKN-TXT-02", Projected("read_element")),
    // Name weicht ab: Lexikon get_readable_document → Tool read_document
    ("AKN-TXT-03", Projected("read_document")),
    ("AKN-TXT-04", Projected("search_text")),
    ("AKN-MOD-01", Projected("get_modifications")),
    (
        "AKN-MOD-02",
        Excluded("G-2: Nutzwert-Lücke, zurückgestellt"),
    ),
    ("AKN-REF-01", Projected("get_references")),
    (
        "AKN-REF-02",
        Excluded("G-2: Nutzwert-Lücke, zurückgestellt"),
    ),
    ("AKN-CMP-01", Projected("list_components")),
    (
        "AKN-CMP-02",
        Excluded("get_component_document — interner Helper, von list_components abgedeckt"),
    ),
    ("AKN-SPC-01", Projected("extract_tables")),
    ("AKN-SPC-02", Projected("detect_foreign_content")),
    ("AKN-CHK-01", Excluded("RAG: semantic-fedlex-Ingest")),
    ("AKN-CHK-02", Excluded("RAG: semantic-fedlex-Ingest")),
];

/// Registrierte Server-Tools **ohne** 1:1-Lexikon-Primitiv (Composite-Tools, die
/// der Reader aus mehreren Primitiven komponiert). Sie haben keine Lexikon-ID und
/// stehen daher nicht in [`MATRIX`], müssen aber bewusst hier verbucht sein —
/// ein neues, nicht eingetragenes Composite-Tool lässt Assertion 3 brechen.
///
/// `compare_versions`: Diff zweier Stichtagsfassungen aus
/// `get_document_structure` + `get_element_text` (Pool `Validation`).
const COMPOSITE_TOOLS: &[&str] = &["compare_versions"];

/// Composite-Tools liegen — anders als jedes lexikon-projizierte Primitiv — **nicht**
/// im Pool `LocalNavigation`/`Discovery`/`JoluxMetadata`, sondern dürfen einen
/// eigenständigen, schwerer privilegierten Pool tragen. `compare_versions` ist
/// `Validation` (nur `Validator` sieht es). Diese Tabelle macht den erwarteten Pool
/// jedes Composite-Tools maschinell prüfbar (Assertion 6).
const COMPOSITE_POOL_OF: &[(&str, ToolPool)] = &[("compare_versions", ToolPool::Validation)];

/// Pools, in denen ein **lexikon-projiziertes** Tool liegen darf. Bewusst **ohne**
/// `Validation`/`Workspace`: ein Lexikon-Primitiv, das versehentlich in einem
/// Composite-/Workspace-Pool landet, ist ein Fehler und soll den Test brechen.
const LEXICON_POOLS: &[ToolPool] = &[
    ToolPool::LocalNavigation,
    ToolPool::LodFederation,
    ToolPool::Discovery,
    ToolPool::JoluxMetadata,
];

// ============================================================
// Hilfsfunktionen
// ============================================================

/// Baut eine Registry mit allen drei Tool-Familien (einmal, von allen Assertions
/// geteilt). Genau die Verdrahtung aus `main.rs`.
fn build_registry() -> Registry {
    let mut r = Registry::new();
    let fetcher = Arc::new(AknFetcher::new(
        MockSparqlClient::from_json(CONS_JSON),
        MockXmlSource::new(MINI_ACT),
        8,
    ));
    register_navigation_tools(&mut r, fetcher);
    register_metadata_tools(&mut r, Arc::new(MockSparqlClient::from_json(EMPTY_JSON)));
    register_discovery_tools(&mut r, Arc::new(MockSparqlClient::from_json(EMPTY_JSON)));
    r
}

/// Die für eine Rolle über `tools/list` sichtbaren Tool-Namen.
fn tool_names_for(reg: &Registry, role: Role) -> BTreeSet<String> {
    reg.list_tools(role)
        .into_iter()
        .filter_map(|v| v.get("name").and_then(|n| n.as_str()).map(String::from))
        .collect()
}

/// Liest beide Lexikon-Dateien und extrahiert alle IDs (`### <ID> · <fn>`).
fn lexicon_ids() -> BTreeSet<String> {
    let root = env!("CARGO_MANIFEST_DIR");
    let re = Regex::new(r"(?m)^### ((?:JLX|AKN)-[A-Z]+-[0-9]+) · ([a-z_]+)").expect("valid regex");
    let mut ids = BTreeSet::new();
    for rel in [
        "/../../docs/10_LEXICON_jolux.md",
        "/../../docs/11_LEXICON_akn.md",
    ] {
        let path = format!("{root}{rel}");
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Lexikon-Datei nicht lesbar: {path} ({e})"));
        for caps in re.captures_iter(&content) {
            ids.insert(caps[1].to_string());
        }
    }
    ids
}

/// Alle Matrix-IDs als Set.
fn matrix_ids() -> BTreeSet<String> {
    MATRIX.iter().map(|(id, _)| id.to_string()).collect()
}

/// Die in der Matrix als `Projected` verbuchten Tool-Namen.
fn matrix_projected_tools() -> BTreeSet<String> {
    MATRIX
        .iter()
        .filter_map(|(_, p)| match p {
            Projected(tool) => Some(tool.to_string()),
            Excluded(_) => None,
        })
        .collect()
}

/// Alle erwartbar registrierten Tool-Namen = projizierte Lexikon-Primitive
/// **plus** die Composite-Tools ohne Lexikon-ID.
fn expected_registered_tools() -> BTreeSet<String> {
    let mut set = matrix_projected_tools();
    set.extend(COMPOSITE_TOOLS.iter().map(|s| s.to_string()));
    set
}

// ============================================================
// Der Test
// ============================================================

#[test]
fn lexicon_and_tool_projection_are_consistent() {
    let lexicon = lexicon_ids();
    let matrix = matrix_ids();
    let reg = build_registry();
    // Validator sieht alle Pools (inkl. `Validation`), also auch das Composite
    // `compare_versions` — die vollständige Menge der registrierten Tools.
    let registered = tool_names_for(&reg, Role::Validator);

    // --- Assertion 1: Lexikon ↔ Matrix deckungsgleich ---
    let missing_in_matrix: Vec<_> = lexicon.difference(&matrix).cloned().collect();
    assert!(
        missing_in_matrix.is_empty(),
        "Lexikon-IDs ohne Matrix-Eintrag (neues Primitiv → Zustand zuordnen!): {missing_in_matrix:?}"
    );
    let stale_in_matrix: Vec<_> = matrix.difference(&lexicon).cloned().collect();
    assert!(
        stale_in_matrix.is_empty(),
        "Matrix-IDs ohne Lexikon-Pendant (veralteter Eintrag entfernen!): {stale_in_matrix:?}"
    );
    assert_eq!(
        lexicon.len(),
        47,
        "Drift-Anker: erwartet 47 Lexikon-IDs, gefunden {}",
        lexicon.len()
    );

    // --- Assertion 2: Jedes `Projected` ist real registriert ---
    let projected = matrix_projected_tools();
    for tool in &projected {
        assert!(
            registered.contains(tool),
            "Matrix verbucht '{tool}' als projiziert, aber es ist nicht registriert"
        );
    }

    // --- Assertion 3: Kein registriertes Tool ist verwaist ---
    // Erwartet = projizierte Lexikon-Primitive ∪ Composite-Tools (ohne Lexikon-ID).
    let expected = expected_registered_tools();
    assert_eq!(
        expected,
        registered,
        "Erwartete Tools (Matrix ∪ Composite) und registrierte Tools (Registry) müssen identisch sein.\n\
         nur erwartet: {:?}\n  nur in Registry (verwaist → Matrix- oder COMPOSITE_TOOLS-Eintrag nötig): {:?}",
        expected.difference(&registered).collect::<Vec<_>>(),
        registered.difference(&expected).collect::<Vec<_>>(),
    );

    // --- Assertion 4: Zähl-Invariante (Doku-Anker) ---
    // 47 Lexikon-IDs: 24 projiziert / 23 ausgeschlossen; + 1 Composite ohne
    // Lexikon-Primitiv ⇒ 25 registrierte MCP-Tools.
    let projected_count = MATRIX
        .iter()
        .filter(|(_, p)| matches!(p, Projected(_)))
        .count();
    let excluded_count = MATRIX
        .iter()
        .filter(|(_, p)| matches!(p, Excluded(_)))
        .count();
    assert_eq!(
        projected_count, 24,
        "erwartet 24 projizierte Lexikon-Primitive"
    );
    assert_eq!(
        excluded_count, 23,
        "erwartet 23 ausgeschlossene Lexikon-Primitive"
    );
    assert_eq!(COMPOSITE_TOOLS.len(), 1, "erwartet genau 1 Composite-Tool");
    assert_eq!(
        registered.len(),
        25,
        "erwartet 25 registrierte MCP-Tools (24 projiziert + 1 Composite)"
    );

    // --- Assertion 5: Pool-Zuordnung stimmt mit der Matrix-Prosa überein ---
    // Jedes projizierte Lexikon-Tool MUSS in einem Lexikon-Pool liegen (nie in
    // Validation/Workspace). So kann ein versehentlich falsch eingeordnetes
    // Primitiv (z.B. Discovery-Tool im Validation-Pool) nicht still durchrutschen.
    for tool in &projected {
        let pool = reg
            .pool_of(tool)
            .unwrap_or_else(|| panic!("projiziertes Tool '{tool}' hat keinen Pool"));
        assert!(
            LEXICON_POOLS.contains(&pool),
            "projiziertes Lexikon-Tool '{tool}' liegt im Pool {pool:?}, erlaubt sind nur {LEXICON_POOLS:?}"
        );
    }
    // Jedes Composite-Tool MUSS in genau dem in COMPOSITE_POOL_OF erwarteten Pool
    // liegen (sichert die „compare_versions ist Validation"-Aussage maschinell ab).
    for (tool, want_pool) in COMPOSITE_POOL_OF {
        let got = reg
            .pool_of(tool)
            .unwrap_or_else(|| panic!("Composite-Tool '{tool}' ist nicht registriert"));
        assert_eq!(
            got, *want_pool,
            "Composite-Tool '{tool}' liegt im Pool {got:?}, erwartet {want_pool:?}"
        );
    }
    assert_eq!(
        COMPOSITE_POOL_OF.len(),
        COMPOSITE_TOOLS.len(),
        "COMPOSITE_POOL_OF und COMPOSITE_TOOLS müssen dieselben Tools führen"
    );

    // --- Assertion 6: RBAC pro Rolle + Monotonie (Least-Privilege) ---
    // Reader ⊆ Navigator ⊆ Validator: jede höhere Rolle sieht mindestens so viel.
    let reader = tool_names_for(&reg, Role::Reader);
    let navigator = tool_names_for(&reg, Role::Navigator);
    assert!(
        reader.is_subset(&navigator),
        "Reader-sichtbare Tools müssen Teilmenge der Navigator-Tools sein; Überschuss: {:?}",
        reader.difference(&navigator).collect::<Vec<_>>()
    );
    assert!(
        navigator.is_subset(&registered),
        "Navigator-sichtbare Tools müssen Teilmenge der Validator-Tools sein; Überschuss: {:?}",
        navigator.difference(&registered).collect::<Vec<_>>()
    );
    // Composite-Tools im Validation-Pool dürfen NUR der Validator sehen — der
    // eigentliche Sinn ihres Sonderstatus (Least-Privilege, ADR-007).
    for tool in COMPOSITE_TOOLS {
        assert!(
            !reader.contains(*tool),
            "Reader darf Composite-Tool '{tool}' (Validation) nicht sehen"
        );
        assert!(
            !navigator.contains(*tool),
            "Navigator darf Composite-Tool '{tool}' (Validation) nicht sehen"
        );
        assert!(
            registered.contains(*tool),
            "Validator muss Composite-Tool '{tool}' sehen"
        );
    }
    // Reader bleibt eng: nur lokaler Cache (LocalNavigation), kein Discovery/
    // JoluxMetadata. Also genau die LocalNavigation-Tools — eine echte Teilmenge.
    assert!(
        !reader.is_empty() && reader.len() < navigator.len(),
        "Reader muss eine echte, nicht-leere Teilmenge sehen (Cache-Lesepfad): \
         reader={}, navigator={}",
        reader.len(),
        navigator.len()
    );
}
