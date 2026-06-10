//! Primitive: Verweise (Lexikon AKN-REF-01/02, Rulebook X8/X11).

use crate::doc::provenance;
use crate::dom::AknDocument;
use crate::error::AknError;
use fedlex_core::{Response, ValidAsOf};
use serde::{Deserialize, Serialize};

/// Ein `<ref>`-Verweis im Dokument.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    /// eId des nächsten eId-tragenden Vorfahren (Verweis-Quelle).
    pub source_eid: Option<String>,
    /// `@href` — 70.8 % absolute Fedlex-URLs, 15 % fehlen ganz (X11.2).
    /// Alle Fedlex-hrefs zeigen auf Work-Ebene (X11.3) — Sprach- und
    /// Datumsauflösung läuft über JOLux (JLX-TMP-02).
    pub href: Option<String>,
    /// Sichtbarer Linktext.
    pub label: String,
}

/// AKN-REF-01: Sammelt alle `<ref>`-Elemente des Dokuments ein
/// (Body, Präambel und Fussnoten — letztere tragen die AS-Verweise
/// der Änderungshistorie).
pub fn get_all_references(
    doc: &AknDocument,
    as_of: ValidAsOf,
) -> Result<Response<Vec<Reference>>, AknError> {
    let prov = provenance(doc, as_of)?;
    let refs = doc
        .find_all(doc.root(), "ref")
        .into_iter()
        .map(|r| Reference {
            source_eid: doc
                .parent(r)
                .and_then(|p| doc.nearest_eid(p))
                .map(str::to_string),
            href: doc.attr(r, "href").map(str::to_string),
            label: doc.text_with_notes(r),
        })
        .collect();
    Ok(Response::new(refs, prov))
}

/// Klassifikation eines href-losen Verweis-Labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefKind {
    /// Interner Artikel-Verweis (`Art. 9a`, `Artikel 7`).
    Article,
    /// SR-Nummer (`101`, `730.0`, `0.814.01`).
    SrNumber,
    /// AS-Fundstelle (`AS 2020 752`).
    AsCitation,
    /// Nicht klassifizierbar — bleibt Text.
    Unknown,
}

/// Ergebnis von [`parse_unlinked_ref`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedRef {
    /// Erkannte Verweis-Art.
    pub kind: RefKind,
    /// Extrahierter Wert (Artikelnummer, SR-Nummer, AS-Fundstelle).
    pub value: String,
}

/// AKN-REF-02: Klassifiziert die 15 % href-losen Verweis-Labels (X11.2)
/// per Heuristik. Reine Textanalyse, bewusst konservativ — was nicht
/// sicher erkennbar ist, bleibt `Unknown` statt falsch verlinkt.
pub fn parse_unlinked_ref(label: &str) -> ParsedRef {
    let t = label.trim();
    // Artikel-Verweise: "Art. 9a", "Artikel 7 Absatz 2".
    for prefix in ["Art.", "Artikel"] {
        if let Some(rest) = t.strip_prefix(prefix) {
            let value = rest.trim().to_string();
            if !value.is_empty() {
                return ParsedRef {
                    kind: RefKind::Article,
                    value,
                };
            }
        }
    }
    // AS-Fundstellen: "AS 2020 752".
    if let Some(rest) = t.strip_prefix("AS ") {
        let rest = rest.trim();
        if rest.len() >= 4 && rest[..4].chars().all(|c| c.is_ascii_digit()) {
            return ParsedRef {
                kind: RefKind::AsCitation,
                value: rest.to_string(),
            };
        }
    }
    // SR-Nummern: nur Ziffern und Punkte ("101", "0.814.01").
    if !t.is_empty()
        && t.chars().all(|c| c.is_ascii_digit() || c == '.')
        && t.chars().any(|c| c.is_ascii_digit())
    {
        return ParsedRef {
            kind: RefKind::SrNumber,
            value: t.to_string(),
        };
    }
    ParsedRef {
        kind: RefKind::Unknown,
        value: t.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdoc::{sample, stichtag};

    #[test]
    fn collects_refs_with_and_without_href() {
        let doc = sample();
        let resp = get_all_references(&doc, stichtag()).unwrap();
        let refs = resp.data();
        // Präambel (2: BV-ref + Note-ref) + Artikel-Note (1) + 2 href-lose in items.
        assert_eq!(refs.len(), 5);
        let with_href = refs.iter().filter(|r| r.href.is_some()).count();
        let without = refs.iter().filter(|r| r.href.is_none()).count();
        assert_eq!(with_href, 3);
        assert_eq!(without, 2);
        let item_ref = refs
            .iter()
            .find(|r| r.source_eid.as_deref() == Some("art_2/para_1/list_1/item_a"))
            .unwrap();
        assert_eq!(item_ref.label, "Art. 1");
        assert!(item_ref.href.is_none());
    }

    #[test]
    fn parses_unlinked_labels() {
        assert_eq!(
            parse_unlinked_ref("Art. 9a"),
            ParsedRef {
                kind: RefKind::Article,
                value: "9a".into()
            }
        );
        assert_eq!(
            parse_unlinked_ref("Artikel 7 Absatz 2").kind,
            RefKind::Article
        );
        assert_eq!(
            parse_unlinked_ref("101"),
            ParsedRef {
                kind: RefKind::SrNumber,
                value: "101".into()
            }
        );
        assert_eq!(parse_unlinked_ref("0.814.01").kind, RefKind::SrNumber);
        assert_eq!(
            parse_unlinked_ref("AS 2020 752"),
            ParsedRef {
                kind: RefKind::AsCitation,
                value: "2020 752".into()
            }
        );
        assert_eq!(parse_unlinked_ref("siehe oben").kind, RefKind::Unknown);
    }
}
