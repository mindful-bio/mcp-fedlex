//! Primitive: Sonderinhalte (Lexikon AKN-SPC-01/02, Rulebook X13/X18).

use crate::dom::AknDocument;
use crate::structure::resolve_eid;
use crate::error::AknError;
use serde::{Deserialize, Serialize};

/// Eine extrahierte Tabelle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableInfo {
    /// eId des nächsten eId-tragenden Vorfahren (oder der Tabelle selbst).
    pub context_eid: Option<String>,
    /// Zeilenzahl.
    pub rows: usize,
    /// Maximale Spaltenzahl.
    pub cols: usize,
    /// Kopfzeile (aus `<th>` der ersten Zeile, sonst leer).
    pub header: Vec<String>,
    /// Datenzeilen als Zelltexte.
    pub data: Vec<Vec<String>>,
    /// `true` ab > 100 Zeilen — als Einheit zu gross für einen Chunk,
    /// muss zeilengruppenweise weiterverarbeitet werden (X13.3).
    pub oversized: bool,
}

/// AKN-SPC-01: Extrahiert Tabellen als strukturierte Einheiten.
///
/// Tabellen nie mitten in der Zeile splitten — sie sind semantische
/// Einheiten (Zuständigkeits-Matrizen, Grenzwert-Listen, X13.2).
/// `within` schränkt auf einen Teilbaum ein.
pub fn extract_tables(
    doc: &AknDocument,
    within: Option<&str>,
) -> Result<Vec<TableInfo>, AknError> {
    let scope = match within {
        Some(eid) => resolve_eid(doc, eid)?.node,
        None => doc.root(),
    };
    let tables = doc
        .find_all(scope, "table")
        .into_iter()
        .map(|t| {
            let trs = doc.find_all(t, "tr");
            let rows = trs.len();
            let mut header = Vec::new();
            let mut data = Vec::new();
            let mut cols = 0;
            for (i, &tr) in trs.iter().enumerate() {
                let ths: Vec<String> = doc
                    .children(tr)
                    .filter(|&c| doc.tag(c) == "th")
                    .map(|c| doc.text_of(c))
                    .collect();
                let tds: Vec<String> = doc
                    .children(tr)
                    .filter(|&c| doc.tag(c) == "td")
                    .map(|c| doc.text_of(c))
                    .collect();
                cols = cols.max(ths.len() + tds.len());
                if i == 0 && !ths.is_empty() {
                    header = ths;
                } else if !tds.is_empty() {
                    data.push(tds);
                }
            }
            TableInfo {
                context_eid: doc.nearest_eid(t).map(str::to_string),
                rows,
                cols,
                header,
                data,
                oversized: rows > 100,
            }
        })
        .collect();
    Ok(tables)
}

/// Art eines `<foreign>`-Inhalts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ForeignKind {
    /// SVG-Grafik (Signets, Schemata).
    Svg,
    /// MathML-Formel — **ohne** MathML-Namespace eingebettet (X18.4),
    /// Erkennung läuft über lokale Tag-Namen.
    MathMl,
    /// SKOS-Vokabular-Fragmente.
    Skos,
    /// Office-OpenXML-Reste (`AlternateContent`).
    Ooxml,
    /// Unbekannter Fremdinhalt.
    Other,
}

/// Ein erkannter `<foreign>`-Block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForeignContent {
    /// eId des nächsten eId-tragenden Vorfahren.
    pub context_eid: Option<String>,
    /// Klassifizierte Art.
    pub kind: ForeignKind,
    /// Anzahl Elemente im Fremdinhalt (Grössenindikator).
    pub element_count: usize,
}

/// AKN-SPC-02: Findet und klassifiziert `<foreign>`-Inseln.
///
/// Der Textfluss der TXT-Primitive schliesst `<foreign>` aus — dieses
/// Primitiv macht die Inseln sichtbar, damit nichts stillschweigend fehlt
/// (Formeln in Energieverordnungen sind normativ relevant).
pub fn detect_foreign_content(doc: &AknDocument) -> Vec<ForeignContent> {
    doc.find_all(doc.root(), "foreign")
        .into_iter()
        .map(|f| {
            let inner = doc.descendants(f);
            let tags: Vec<&str> = inner.iter().skip(1).map(|&d| doc.tag(d)).collect();
            let has = |names: &[&str]| tags.iter().any(|t| names.contains(t));
            let kind = if has(&["svg", "path", "rect", "g", "circle"]) {
                ForeignKind::Svg
            } else if has(&[
                "math", "mrow", "mi", "mo", "mn", "mfrac", "msub", "msup", "mtext",
            ]) {
                ForeignKind::MathMl
            } else if has(&["prefLabel", "altLabel", "notation", "Concept"]) {
                ForeignKind::Skos
            } else if has(&["AlternateContent", "Choice", "Fallback"]) {
                ForeignKind::Ooxml
            } else {
                ForeignKind::Other
            };
            ForeignContent {
                context_eid: doc.nearest_eid(f).map(str::to_string),
                kind,
                element_count: tags.len(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdoc::sample;

    #[test]
    fn extracts_table_with_header_and_data() {
        let doc = sample();
        let tables = extract_tables(&doc, None).unwrap();
        assert_eq!(tables.len(), 1);
        let t = &tables[0];
        assert_eq!(t.context_eid.as_deref(), Some("art_2/para_1/tbl_1"));
        assert_eq!(t.rows, 2);
        assert_eq!(t.cols, 2);
        assert_eq!(t.header, ["Jahr", "GWh"]);
        assert_eq!(t.data, [["2035", "37400"]]);
        assert!(!t.oversized);
    }

    #[test]
    fn scoped_table_extraction() {
        let doc = sample();
        assert_eq!(extract_tables(&doc, Some("art_2")).unwrap().len(), 1);
        assert!(extract_tables(&doc, Some("art_1")).unwrap().is_empty());
    }

    #[test]
    fn detects_mathml_without_namespace() {
        let doc = sample();
        let foreign = detect_foreign_content(&doc);
        assert_eq!(foreign.len(), 1);
        // X18.4: MathML ohne Namespace, Erkennung über lokale Namen.
        assert_eq!(foreign[0].kind, ForeignKind::MathMl);
        assert!(foreign[0].element_count >= 3);
        assert_eq!(foreign[0].context_eid.as_deref(), Some("art_2/para_1"));
    }
}
