//! Primitive: Dokument-Identität & Muster (Lexikon AKN-DOC-02/03, Rulebook X2/X4/X19).

use crate::dom::{AknDocument, NodeId};
use crate::error::AknError;
use fedlex_core::{Eli, Provenance, TransactionTime, ValidAsOf};
use serde::{Deserialize, Serialize};

/// FRBR-Selbstauskunft einer AKN-Datei (AKN-DOC-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrbrMetadata {
    /// Work-URI, z.B. `/eli/cc/2017/762`.
    pub eli_work: String,
    /// Expression-URI (Sprachfassung), z.B. `/eli/cc/2017/762/de`.
    pub eli_expression: Option<String>,
    /// Manifestation-URI (Format), z.B. `/eli/cc/2017/762/de/xml`.
    pub eli_manifestation: Option<String>,
    /// SR-Nummer aus `FRBRnumber`, z.B. `SR 730.0`.
    pub sr_number: Option<String>,
    /// Titel aus `FRBRname`.
    pub title: Option<String>,
    /// Sprache aus `FRBRlanguage/@language`.
    pub language: Option<String>,
    /// `FRBRdate`-Paare `(name, datum)` — Namen sind jolux-spezifisch
    /// (`jolux:dateDocument` 100 %, X19.4).
    pub dates: Vec<(String, String)>,
    /// Autor, aufgelöst über `FRBRauthor/@href` → `TLCOrganization/@showAs`.
    pub author: Option<String>,
    /// Anzahl `FRBRExpression`-Blöcke — 17.1 % der Dateien haben mehrere (X17.6).
    pub expression_count: usize,
}

/// AKN-DOC-02: Liest die 7 Kern-Metadaten aus dem ersten
/// `<identification>`-Block (in 100 % der Corpus-Dateien vorhanden, X2.2).
///
/// `FRBRauthor`/`FRBRdate` sind auf Work und Expression gespiegelt (X19.3) —
/// gelesen wird die Work-Ebene. Bei mehreren `FRBRExpression`-Blöcken
/// (konsolidierte Fassungen) zählt `expression_count` alle, die URI stammt
/// aus dem ersten.
pub fn get_frbr_metadata(doc: &AknDocument) -> Result<FrbrMetadata, AknError> {
    let meta = doc
        .find_child(doc.root(), "meta")
        .ok_or(AknError::MissingFrbr)?;
    let ident = doc
        .find_child(meta, "identification")
        .ok_or(AknError::MissingFrbr)?;

    let value_of = |parent: NodeId, tag: &str| -> Option<String> {
        doc.find_child(parent, tag)
            .and_then(|n| doc.attr(n, "value"))
            .map(str::to_string)
    };

    let work = doc.find_child(ident, "FRBRWork").ok_or(AknError::MissingFrbr)?;
    let eli_work = value_of(work, "FRBRuri")
        .or_else(|| value_of(work, "FRBRthis"))
        .ok_or(AknError::MissingFrbr)?;

    let expressions = doc.find_all(ident, "FRBRExpression");
    let first_expr = expressions.first().copied();
    let eli_expression = first_expr.and_then(|e| {
        value_of(e, "FRBRuri").or_else(|| value_of(e, "FRBRthis"))
    });
    let language = first_expr
        .and_then(|e| doc.find_child(e, "FRBRlanguage"))
        .and_then(|n| doc.attr(n, "language"))
        .map(str::to_string);

    // Live-Befund (2026-06-10): FRBRname ist mehrsprachig (rm/it/de/fr als
    // Geschwister) — den Titel der Expression-Sprache wählen, sonst den ersten.
    let names: Vec<NodeId> = doc
        .children(work)
        .filter(|&n| doc.tag(n) == "FRBRname")
        .collect();
    let title = names
        .iter()
        .find(|&&n| doc.attr(n, "lang") == language.as_deref())
        .or_else(|| names.first())
        .and_then(|&n| doc.attr(n, "value"))
        .map(str::to_string);

    let eli_manifestation = doc
        .find_child(ident, "FRBRManifestation")
        .and_then(|m| value_of(m, "FRBRuri").or_else(|| value_of(m, "FRBRthis")));

    let dates = doc
        .find_all(work, "FRBRdate")
        .into_iter()
        .filter_map(|d| {
            Some((
                doc.attr(d, "name")?.to_string(),
                doc.attr(d, "date")?.to_string(),
            ))
        })
        .collect();

    // Autor: Anker `#ch.bk` → TLCOrganization mit eId `ch.bk` (X19.5).
    let author = doc
        .find_child(work, "FRBRauthor")
        .and_then(|a| doc.attr(a, "href"))
        .map(|href| href.trim_start_matches('#').to_string())
        .and_then(|anchor| {
            doc.find_all(meta, "TLCOrganization")
                .into_iter()
                .find(|&org| doc.eid(org) == Some(anchor.as_str()))
                .and_then(|org| doc.attr(org, "showAs"))
                .map(str::to_string)
        });

    Ok(FrbrMetadata {
        eli_work,
        eli_expression,
        eli_manifestation,
        sr_number: value_of(work, "FRBRnumber"),
        title,
        language,
        dates,
        author,
        expression_count: expressions.len(),
    })
}

/// Die 5+1 Dokumentmuster des Corpus (X4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocPattern {
    /// Kapitel/Abschnitte + Artikel (2.8 %, dominant bei strukturierten CC).
    Structured,
    /// Nur Artikel ohne Obergliederung (23.5 %, CC zu 74 %).
    FlatArticles,
    /// Generische `<level>`-Hierarchie (34.9 %, OC zu 68.3 %).
    LevelBased,
    /// Kein Body — Metadaten-Stub (34.0 %, FGA zu 50.7 %).
    NoBody,
    /// Nur `<mod>`-Änderungsblöcke (3.4 %).
    Amendment,
    /// Body ohne Artikel/Level/Mod (1.4 %).
    Other,
}

/// Klassifikations-Ergebnis von AKN-DOC-03.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatternInfo {
    /// Erkanntes Muster — die Strategie-Weiche für STR/TXT/CHK.
    pub pattern: DocPattern,
    /// Body vorhanden? (34 % der Dateien nicht, X1.2)
    pub has_body: bool,
    /// Anzahl `<article>` im Body.
    pub article_count: usize,
    /// Anzahl `<level>` im Body.
    pub level_count: usize,
    /// Anzahl `<mod>` im Body.
    pub mod_count: usize,
    /// Anzahl `<component>` (Anhänge als eigene FRBR-Werke, X19.8).
    pub component_count: usize,
}

/// AKN-DOC-03: Bestimmt das Dokumentmuster — **vor** jedem Text-/Struktur-
/// Zugriff aufrufen. 91 % der OC/FGA-Dateien haben keine Artikel (X5.2),
/// wer blind Artikel-Primitive ruft, hält leere Antworten für Fehler.
pub fn classify_pattern(doc: &AknDocument) -> PatternInfo {
    // act-Ebene hat nur <body>; <mainBody> existiert nur in Component-Docs
    // (X4.1, GAP I1) — beide unterstützen, da CMP-02 Teilbäume liefert.
    let body = doc
        .find_child(doc.root(), "body")
        .or_else(|| doc.find_child(doc.root(), "mainBody"));
    let component_count = doc.find_all(doc.root(), "component").len();

    let Some(body) = body else {
        return PatternInfo {
            pattern: DocPattern::NoBody,
            has_body: false,
            article_count: 0,
            level_count: 0,
            mod_count: 0,
            component_count,
        };
    };

    let article_count = doc.find_all(body, "article").len();
    let level_count = doc.find_all(body, "level").len();
    let mod_count = doc.find_all(body, "mod").len();
    let structured_markers = ["chapter", "section", "title", "part", "book"]
        .iter()
        .map(|t| doc.find_all(body, t).len())
        .sum::<usize>();

    let pattern = if article_count > 0 && structured_markers > 0 {
        DocPattern::Structured
    } else if article_count > 0 {
        DocPattern::FlatArticles
    } else if level_count > 0 {
        DocPattern::LevelBased
    } else if mod_count > 0 {
        DocPattern::Amendment
    } else {
        DocPattern::Other
    };

    PatternInfo {
        pattern,
        has_body: true,
        article_count,
        level_count,
        mod_count,
        component_count,
    }
}

/// Extrahiert den `eli/…`-Pfad aus einer FRBR-URI.
///
/// Live-Befund (2026-06-10): Konsolidierungs-XML trägt **absolute, datierte**
/// Work-URIs (`https://fedlex.data.admin.ch/eli/cc/2017/762/20260401`) — der
/// Analyse-Snapshot zeigte relative (`/eli/cc/2017/762`). Beide Formen
/// normalisieren auf den Pfad ab `eli/`.
pub(crate) fn eli_path(uri: &str) -> Option<&str> {
    if let Some(idx) = uri.find("/eli/") {
        Some(&uri[idx + 1..])
    } else if uri.starts_with("eli/") {
        Some(uri)
    } else {
        None
    }
}

/// Reduziert eine (möglicherweise datierte) Konsolidierungs-URI auf die
/// **Work-Ebene**: ein abschliessendes 8-stelliges Datums-Segment
/// (`…/762/20260401` → `…/762`) wird entfernt.
///
/// Begründung: Provenance-ELI und Chunk-IDs müssen über Konsolidierungen
/// hinweg stabil bleiben und mit der JOLux-Schicht joinbar sein (dort ist
/// alles Work-Ebene, X11.3/J-Konvention). Der Stichtag steckt in `ValidAsOf`
/// bzw. im Chunk-Feld `date`, nicht in der ELI.
pub(crate) fn work_eli_path(uri: &str) -> Option<&str> {
    let path = eli_path(uri)?;
    match path.rsplit_once('/') {
        Some((head, last)) if last.len() == 8 && last.chars().all(|c| c.is_ascii_digit()) => {
            Some(head)
        }
        _ => Some(path),
    }
}

/// Baut die Provenance einer Rechtsaussage aus der FRBR-Selbstauskunft
/// (ADR-004). Der Stichtag kommt vom Aufrufer — er hat die Manifestation
/// über JLX-TMP-02/RES-04 aufgelöst. Die ELI ist Work-Ebene (datumslos).
pub(crate) fn provenance(doc: &AknDocument, as_of: ValidAsOf) -> Result<Provenance, AknError> {
    let meta = get_frbr_metadata(doc)?;
    let path = work_eli_path(&meta.eli_work).unwrap_or(&meta.eli_work);
    let eli = Eli::new(path)?;
    Ok(Provenance::new(eli, as_of, TransactionTime::now()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdoc::SAMPLE_ACT;

    #[test]
    fn frbr_metadata_extracts_core_fields() {
        let doc = AknDocument::parse(SAMPLE_ACT).unwrap();
        let m = get_frbr_metadata(&doc).unwrap();
        assert_eq!(m.eli_work, "/eli/cc/2017/762");
        assert_eq!(m.eli_expression.as_deref(), Some("/eli/cc/2017/762/de"));
        assert_eq!(m.sr_number.as_deref(), Some("SR 730.0"));
        assert_eq!(m.title.as_deref(), Some("Energiegesetz"));
        assert_eq!(m.language.as_deref(), Some("de"));
        assert_eq!(m.author.as_deref(), Some("Bundeskanzlei"));
        assert_eq!(m.expression_count, 1);
        assert!(m
            .dates
            .iter()
            .any(|(n, d)| n == "jolux:dateDocument" && d == "2018-01-01"));
    }

    #[test]
    fn classifies_structured_pattern() {
        let doc = AknDocument::parse(SAMPLE_ACT).unwrap();
        let info = classify_pattern(&doc);
        assert_eq!(info.pattern, DocPattern::Structured);
        assert!(info.has_body);
        assert_eq!(info.article_count, 3);
        assert_eq!(info.component_count, 1);
    }

    #[test]
    fn classifies_no_body_as_stub() {
        let xml = r#"<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0">
          <act name="publicLaw"><meta><identification>
            <FRBRWork><FRBRuri value="/eli/cc/1900/1"/></FRBRWork>
          </identification></meta><preface><p><docTitle>Alter Stub</docTitle></p></preface></act>
        </akomaNtoso>"#;
        let doc = AknDocument::parse(xml).unwrap();
        let info = classify_pattern(&doc);
        // X1.2: 34 % des Corpus sind body-lose Metadaten-Stubs.
        assert_eq!(info.pattern, DocPattern::NoBody);
        assert!(!info.has_body);
    }

    #[test]
    fn work_eli_path_strips_consolidation_date() {
        // Live-Form: absolute, datierte Konsolidierungs-URI.
        assert_eq!(
            work_eli_path("https://fedlex.data.admin.ch/eli/cc/2017/762/20260401"),
            Some("eli/cc/2017/762")
        );
        // Undatierte Formen bleiben unberührt.
        assert_eq!(work_eli_path("/eli/cc/2017/762"), Some("eli/cc/2017/762"));
        assert_eq!(work_eli_path("/eli/oc/2024/679"), Some("eli/oc/2024/679"));
        // Component-ELI mit Suffix-Segment bleibt unberührt.
        assert_eq!(
            work_eli_path("/eli/cc/2017/762/anx_1"),
            Some("eli/cc/2017/762/anx_1")
        );
    }
}
