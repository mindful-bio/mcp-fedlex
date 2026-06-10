//! Primitive: Anhänge als Components (Lexikon AKN-CMP-01/02, Rulebook X10/X19).

use crate::dom::{AknDocument, NodeId};
use crate::error::AknError;
use serde::{Deserialize, Serialize};

/// Beschreibung eines `<component>`-Anhangs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentInfo {
    /// Position innerhalb `<components>` (0-basiert).
    pub index: usize,
    /// `@name` des inneren `<doc>` (typisch `annex`).
    pub doc_name: Option<String>,
    /// Work-URI des Anhangs — Components sind **eigene FRBR-Werke** mit
    /// eigenem ELI (X19.8).
    pub eli_work: Option<String>,
    /// Titel aus dem Component-eigenen `FRBRname`.
    pub title: Option<String>,
    /// `true`, wenn der `mainBody` praktisch leer ist (< 100 Zeichen) —
    /// Verweis-Stub statt Inhalt (X10-Befund).
    pub is_empty_stub: bool,
}

/// AKN-CMP-01: Listet die Anhänge eines Dokuments auf.
///
/// Components haben einen vollständigen eigenen FRBR-Block — die Metadaten
/// hier stammen aus dem Component, nicht vom Hauptdokument.
pub fn list_components(doc: &AknDocument) -> Vec<ComponentInfo> {
    component_docs(doc)
        .into_iter()
        .enumerate()
        .map(|(index, inner)| {
            let meta = inner.and_then(|d| doc.find_child(d, "meta"));
            let work = meta
                .and_then(|m| doc.find_child(m, "identification"))
                .and_then(|i| doc.find_child(i, "FRBRWork"));
            let value_of = |tag: &str| -> Option<String> {
                work.and_then(|w| doc.find_child(w, tag))
                    .and_then(|n| doc.attr(n, "value"))
                    .map(str::to_string)
            };
            let body_len = inner
                .and_then(|d| doc.find_child(d, "mainBody"))
                .map(|b| doc.text_of(b).chars().count())
                .unwrap_or(0);
            ComponentInfo {
                index,
                doc_name: inner.and_then(|d| doc.attr(d, "name")).map(str::to_string),
                eli_work: value_of("FRBRuri").or_else(|| value_of("FRBRthis")),
                title: value_of("FRBRname"),
                is_empty_stub: body_len < 100,
            }
        })
        .collect()
}

/// AKN-CMP-02: Extrahiert einen Anhang als eigenständiges Dokument.
///
/// Das Ergebnis ist ein vollwertiges [`AknDocument`] mit `<doc>`-Wurzel —
/// alle anderen Primitive (DOC-02/03, STR, TXT, CHK) funktionieren darauf.
/// Der Body heisst dort `<mainBody>` (X4.1), die Primitive kennen beide.
pub fn get_component_document(doc: &AknDocument, index: usize) -> Result<AknDocument, AknError> {
    let docs = component_docs(doc);
    match docs.get(index) {
        Some(Some(inner)) => Ok(doc.subtree_document(*inner)),
        _ => Err(AknError::ComponentNotFound {
            index,
            available: docs.len(),
        }),
    }
}

fn component_docs(doc: &AknDocument) -> Vec<Option<NodeId>> {
    doc.find_all(doc.root(), "component")
        .into_iter()
        .map(|c| doc.find_child(c, "doc"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{classify_pattern, get_frbr_metadata};
    use crate::testdoc::sample;

    #[test]
    fn lists_components_with_own_frbr() {
        let doc = sample();
        let comps = list_components(&doc);
        assert_eq!(comps.len(), 1);
        assert_eq!(comps[0].doc_name.as_deref(), Some("annex"));
        // X19.8: eigener ELI, nicht der des Hauptdokuments.
        assert_eq!(comps[0].eli_work.as_deref(), Some("/eli/cc/2017/762/anx_1"));
        assert_eq!(comps[0].title.as_deref(), Some("Anhang 1"));
        assert!(!comps[0].is_empty_stub);
    }

    #[test]
    fn component_document_is_fully_functional() {
        let doc = sample();
        let annex = get_component_document(&doc, 0).unwrap();
        assert_eq!(annex.tag(annex.root()), "doc");
        // DOC-Primitive funktionieren auf dem Teilbaum.
        let m = get_frbr_metadata(&annex).unwrap();
        assert_eq!(m.eli_work, "/eli/cc/2017/762/anx_1");
        // mainBody wird als Body erkannt (X4.1).
        let info = classify_pattern(&annex);
        assert!(info.has_body);
        assert_eq!(info.level_count, 1);
    }

    #[test]
    fn out_of_range_index_errors() {
        let doc = sample();
        let err = get_component_document(&doc, 5).unwrap_err();
        assert!(matches!(
            err,
            AknError::ComponentNotFound {
                index: 5,
                available: 1
            }
        ));
    }
}
