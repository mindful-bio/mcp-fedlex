//! Primitive: Text-Extraktion & Suche (Lexikon AKN-TXT-01/02/04, Rulebook X6/X18).

use crate::dom::AknDocument;
use crate::doc::provenance;
use crate::error::AknError;
use crate::structure::{resolve_eid, section_path_of};
use fedlex_core::{Response, ValidAsOf};
use serde::{Deserialize, Serialize};

/// Ein `<ref>`-Ziel innerhalb einer Fussnote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefTarget {
    /// `@href` — fehlt bei 15 % der refs (X11.2).
    pub href: Option<String>,
    /// Sichtbarer Linktext.
    pub label: String,
}

/// Eine redaktionelle Fussnote (`<authorialNote>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    /// `@marker` (Fussnotenzeichen).
    pub marker: Option<String>,
    /// Fussnotentext (inkl. Linktexte).
    pub text: String,
    /// AS-/SR-Verweise in der Note — 71.3 % der Notes tragen welche (X12.4),
    /// das ist die Brücke zur JOLux-Änderungshistorie.
    pub refs: Vec<RefTarget>,
}

/// Volltext eines Elements mit getrennten Fussnoten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementText {
    /// Aufgelöste eId (Dokument-Schreibweise).
    pub eid: String,
    /// Element-Typ.
    pub kind: String,
    /// Nummer aus `<num>`.
    pub num: Option<String>,
    /// Überschrift aus `<heading>`.
    pub heading: Option<String>,
    /// Normtext — Fussnoten und `<foreign>` ausgeschlossen (X6.4, X18.4).
    pub text: String,
    /// Separat eingesammelte Fussnoten.
    pub notes: Vec<Note>,
    /// Gliederungs-Pfad als eId-Liste (Chunk-Kontext, X14.3).
    pub section_path: Vec<String>,
}

/// AKN-TXT-01: Volltext eines Artikels.
///
/// Erzwingt `<article>` — für andere Elemente [`get_element_text`] nutzen.
/// Vor dem Aufruf das Muster prüfen (AKN-DOC-03), denn 91 % der OC/FGA
/// haben gar keine Artikel (X5.2).
pub fn get_article_text(
    doc: &AknDocument,
    eid: &str,
    as_of: ValidAsOf,
) -> Result<Response<ElementText>, AknError> {
    let hit = resolve_eid(doc, eid)?;
    let tag = doc.tag(hit.node);
    if tag != "article" {
        return Err(AknError::WrongElementKind {
            eid: eid.to_string(),
            found: tag.to_string(),
            expected: "article",
        });
    }
    get_element_text(doc, eid, as_of)
}

/// AKN-TXT-02: Volltext eines beliebigen eId-Elements (level, chapter, …).
/// Das Arbeitstier für die 35 % LEVEL_BASED-Dokumente.
pub fn get_element_text(
    doc: &AknDocument,
    eid: &str,
    as_of: ValidAsOf,
) -> Result<Response<ElementText>, AknError> {
    let prov = provenance(doc, as_of)?;
    let hit = resolve_eid(doc, eid)?;
    let node = hit.node;
    let notes = collect_notes(doc, node);
    let section_path = section_path_of(doc, node)
        .into_iter()
        .filter_map(|s| s.eid)
        .collect();
    Ok(Response::new(
        ElementText {
            eid: doc.eid(node).unwrap_or(eid).to_string(),
            kind: doc.tag(node).to_string(),
            num: doc.find_child(node, "num").map(|n| doc.text_of(n)),
            heading: doc.find_child(node, "heading").map(|h| doc.text_of(h)),
            text: doc.text_of(node),
            notes,
            section_path,
        },
        prov,
    ))
}

/// Fussnoten eines Teilbaums einsammeln (intern, auch von MOD-02 genutzt).
pub(crate) fn collect_notes(doc: &AknDocument, node: crate::dom::NodeId) -> Vec<Note> {
    doc.find_all(node, "authorialNote")
        .into_iter()
        .map(|n| Note {
            marker: doc.attr(n, "marker").map(str::to_string),
            text: doc.text_with_notes(n),
            refs: doc
                .find_all(n, "ref")
                .into_iter()
                .map(|r| RefTarget {
                    href: doc.attr(r, "href").map(str::to_string),
                    label: doc.text_with_notes(r),
                })
                .collect(),
        })
        .collect()
}

/// Ein Suchtreffer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    /// eId des Blatt-Elements mit dem Treffer.
    pub eid: String,
    /// Element-Typ.
    pub kind: String,
    /// Textausschnitt um den Treffer (±60 Zeichen).
    pub snippet: String,
}

/// AKN-TXT-04: Case-insensitive Volltextsuche über die eId-Blätter des
/// Dokuments (Hollowing-Sicht, X20 — Eltern-Container würden jeden Treffer
/// vervielfachen). Kein Ersatz für eine Such-Infrastruktur, aber das
/// deterministische Primitiv darunter.
pub fn search_text(doc: &AknDocument, query: &str, max_hits: usize) -> Vec<SearchHit> {
    let needle = query.to_lowercase();
    let mut hits = Vec::new();
    if needle.is_empty() {
        return hits;
    }
    let mut leaf_eids: Vec<(&str, crate::dom::NodeId)> = doc
        .all_eids()
        .flat_map(|(eid, nodes)| nodes.iter().map(move |&n| (eid, n)))
        .filter(|&(_, n)| !has_eid_descendant(doc, n))
        .collect();
    // Dokumentreihenfolge statt HashMap-Zufall.
    leaf_eids.sort_unstable_by_key(|&(_, n)| n);
    for (eid, node) in leaf_eids {
        if hits.len() >= max_hits {
            break;
        }
        let text = doc.text_of(node);
        let lower = text.to_lowercase();
        if let Some(pos) = lower.find(&needle) {
            hits.push(SearchHit {
                eid: eid.to_string(),
                kind: doc.tag(node).to_string(),
                snippet: snippet_around(&text, pos, needle.len()),
            });
        }
    }
    hits
}

pub(crate) fn has_eid_descendant(doc: &AknDocument, node: crate::dom::NodeId) -> bool {
    doc.descendants(node)
        .into_iter()
        .skip(1)
        .any(|d| doc.eid(d).is_some())
}

fn snippet_around(text: &str, pos: usize, len: usize) -> String {
    let start = text[..pos]
        .char_indices()
        .rev()
        .nth(59)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let end_base = pos + len;
    let end = text[end_base..]
        .char_indices()
        .nth(60)
        .map(|(i, _)| end_base + i)
        .unwrap_or(text.len());
    text[start..end].replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdoc::{sample, stichtag};

    #[test]
    fn article_text_separates_notes_from_norm_text() {
        let doc = sample();
        let resp = get_article_text(&doc, "art_1", stichtag()).unwrap();
        let t = resp.data();
        assert_eq!(t.kind, "article");
        assert_eq!(t.num.as_deref(), Some("Art. 1"));
        assert_eq!(t.heading.as_deref(), Some("Zweck"));
        assert!(t.text.contains("Energieversorgung"));
        // Fussnote getrennt, nicht im Normtext (X6.4).
        assert!(!t.text.contains("AS 2020 752"));
        assert_eq!(t.notes.len(), 1);
        assert!(t.notes[0].text.contains("AS 2020 752"));
        assert_eq!(
            t.notes[0].refs[0].href.as_deref(),
            Some("https://fedlex.data.admin.ch/eli/oc/2020/752")
        );
        assert_eq!(t.section_path, ["chap_1", "art_1"]);
    }

    #[test]
    fn article_text_rejects_non_article() {
        let doc = sample();
        let err = get_article_text(&doc, "chap_1", stichtag()).unwrap_err();
        assert!(matches!(err, AknError::WrongElementKind { .. }));
    }

    #[test]
    fn element_text_works_for_levels() {
        let doc = sample();
        let resp = get_element_text(&doc, "lvl_u1", stichtag()).unwrap();
        assert_eq!(resp.data().kind, "level");
        assert!(resp.data().text.contains("Levelinhalt"));
    }

    #[test]
    fn search_finds_leaf_with_snippet() {
        let doc = sample();
        let hits = search_text(&doc, "energieversorgung", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].eid, "art_1/para_1");
        assert!(hits[0].snippet.contains("Energieversorgung"));
        assert!(search_text(&doc, "gibtesnicht", 10).is_empty());
    }
}
