//! Primitive: Änderungserlasse & Änderungshistorie (Lexikon AKN-MOD-01/02, Rulebook X7/X12).

use crate::doc::provenance;
use crate::dom::AknDocument;
use crate::error::AknError;
use crate::structure::resolve_eid;
use crate::text::{collect_notes, RefTarget};
use fedlex_core::{Response, ValidAsOf};
use serde::{Deserialize, Serialize};

/// Ein `<mod>`-Änderungsblock mit seinem zitierten neuen Wortlaut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Modification {
    /// eId des `<mod>`-Elements.
    pub mod_eid: Option<String>,
    /// Element-Typ der zitierten Wurzel (95.6 % `paragraph`, X7.3).
    pub quoted_root_kind: Option<String>,
    /// eId der zitierten Wurzel — verweist in die Struktur des
    /// **geänderten** Gesetzes, nicht des Änderungserlasses.
    pub quoted_eid: Option<String>,
    /// Neuer Wortlaut (Normtext, Fussnoten ausgeschlossen).
    pub new_text: String,
}

/// AKN-MOD-01: Extrahiert alle Änderungsblöcke eines Änderungserlasses (OC).
///
/// Verlässliche 1:1-Invariante — jeder `<mod>` hat genau eine
/// `<quotedStructure>` (47'561:47'561 im Corpus, X7.2). Leer bei
/// Konsolidierungen (CC), die Mods sind dort bereits eingearbeitet.
pub fn get_modifications(
    doc: &AknDocument,
    as_of: ValidAsOf,
) -> Result<Response<Vec<Modification>>, AknError> {
    let prov = provenance(doc, as_of)?;
    let mods = doc
        .find_all(doc.root(), "mod")
        .into_iter()
        .map(|m| {
            let quoted = doc.find_all(m, "quotedStructure").into_iter().next();
            let root = quoted.and_then(|q| doc.children(q).next());
            Modification {
                mod_eid: doc.eid(m).map(str::to_string),
                quoted_root_kind: root.map(|r| doc.tag(r).to_string()),
                quoted_eid: root.and_then(|r| doc.eid(r)).map(str::to_string),
                new_text: quoted.map(|q| doc.text_of(q)).unwrap_or_default(),
            }
        })
        .collect();
    Ok(Response::new(mods, prov))
}

/// Eine Fussnote der Änderungshistorie, verankert an ihrem Norm-Element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeNote {
    /// eId des nächsten eId-tragenden Vorfahren — die Fussnote dokumentiert
    /// die Änderungsgeschichte dieses Elements.
    pub anchor_eid: Option<String>,
    /// Fussnotenzeichen.
    pub marker: Option<String>,
    /// Fussnotentext ("Fassung gemäss …", "Eingefügt durch …").
    pub text: String,
    /// AS-/SR-Verweise — die maschinenlesbare Brücke zur JOLux-Historie
    /// (JLX-HIS-01), 71.3 % der Notes tragen welche (X12.4).
    pub refs: Vec<RefTarget>,
}

/// AKN-MOD-02: Sammelt die Änderungshistorie-Fussnoten ein.
///
/// `within` schränkt auf einen Teilbaum ein (z.B. einen Artikel),
/// `None` nimmt das ganze Dokument. `@placement` ist im Corpus zu 100 %
/// abwesend (X12.2) — die Position ergibt sich aus dem Anker.
pub fn extract_change_notes(
    doc: &AknDocument,
    within: Option<&str>,
    as_of: ValidAsOf,
) -> Result<Response<Vec<ChangeNote>>, AknError> {
    let prov = provenance(doc, as_of)?;
    let scope = match within {
        Some(eid) => resolve_eid(doc, eid)?.node,
        None => doc.root(),
    };
    let notes = doc
        .find_all(scope, "authorialNote")
        .into_iter()
        .map(|n| {
            let parent = doc.parent(n);
            let anchor = parent.and_then(|p| doc.nearest_eid(p)).map(str::to_string);
            // Marker/Text/Refs über den geteilten Sammler (TXT-Schicht).
            let mut collected = collect_notes(doc, n);
            let note = collected.pop().unwrap_or(crate::text::Note {
                marker: None,
                text: String::new(),
                refs: Vec::new(),
            });
            ChangeNote {
                anchor_eid: anchor,
                marker: note.marker,
                text: note.text,
                refs: note.refs,
            }
        })
        .collect();
    Ok(Response::new(notes, prov))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdoc::{sample, stichtag};

    #[test]
    fn mod_has_exactly_one_quoted_structure() {
        let doc = sample();
        let resp = get_modifications(&doc, stichtag()).unwrap();
        let mods = resp.data();
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].mod_eid.as_deref(), Some("mod_1"));
        // X7.3: zitierte Wurzel ist zu 95.6 % ein paragraph.
        assert_eq!(mods[0].quoted_root_kind.as_deref(), Some("paragraph"));
        assert_eq!(mods[0].quoted_eid.as_deref(), Some("mod_1/quot_1/para_3"));
        assert!(mods[0].new_text.contains("Neuer Wortlaut"));
    }

    #[test]
    fn change_notes_are_anchored_and_carry_refs() {
        let doc = sample();
        let resp = extract_change_notes(&doc, Some("art_1"), stichtag()).unwrap();
        let notes = resp.data();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].anchor_eid.as_deref(), Some("art_1/para_2"));
        assert!(notes[0].text.contains("21. Juni 2019"));
        assert_eq!(notes[0].refs.len(), 1);
        assert!(notes[0].refs[0]
            .href
            .as_deref()
            .unwrap()
            .contains("eli/oc/2020/752"));
    }

    #[test]
    fn change_notes_whole_doc_includes_preamble() {
        let doc = sample();
        let resp = extract_change_notes(&doc, None, stichtag()).unwrap();
        // Präambel-Note + Artikel-Note.
        assert_eq!(resp.data().len(), 2);
    }
}
