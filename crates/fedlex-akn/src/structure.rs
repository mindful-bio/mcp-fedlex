//! Primitive: Struktur-Navigation (Lexikon AKN-STR-01/02/03, Rulebook X5/X9/X15).
//!
//! Die Vollstruktur, die JOLux nicht hat (dort max. 8.5 % Abdeckung, J4.1).

use crate::dom::{is_hierarchy_tag, normalize_eid, AknDocument, NodeId};
use crate::doc::provenance;
use crate::error::AknError;
use fedlex_core::{Response, ValidAsOf};
use serde::{Deserialize, Serialize};

/// Ein Knoten der Dokument-Gliederung.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineNode {
    /// eId, sofern vorhanden.
    pub eid: Option<String>,
    /// Element-Typ (`article`, `chapter`, `level`, …).
    pub kind: String,
    /// Nummer aus `<num>` (z.B. `Art. 1`).
    pub num: Option<String>,
    /// Überschrift aus `<heading>`.
    pub heading: Option<String>,
    /// Untergeordnete Gliederungselemente.
    pub children: Vec<OutlineNode>,
}

/// AKN-STR-01: Liefert die Gliederung des Dokuments (Inhaltsverzeichnis).
///
/// `type_filter` (z.B. `Some("article")`) flacht das Ergebnis auf alle
/// Elemente dieses Typs ab — das ist zugleich die Artikel-Enumeration.
/// Bei LEVEL_BASED-Dokumenten (35 %) heisst die Gliederung `<level>` mit
/// generischen eIds — Überschriften tragen dort die Semantik.
pub fn get_document_structure(
    doc: &AknDocument,
    type_filter: Option<&str>,
    as_of: ValidAsOf,
) -> Result<Response<Vec<OutlineNode>>, AknError> {
    let prov = provenance(doc, as_of)?;
    let body = doc
        .find_child(doc.root(), "body")
        .or_else(|| doc.find_child(doc.root(), "mainBody"));
    let tree = match body {
        Some(b) => outline_children(doc, b),
        None => Vec::new(), // NO_BODY-Stub (X1.2) — leere Struktur ist Datum, kein Fehler.
    };
    let result = match type_filter {
        Some(t) => flatten_filter(&tree, t),
        None => tree,
    };
    Ok(Response::new(result, prov))
}

fn outline_children(doc: &AknDocument, id: NodeId) -> Vec<OutlineNode> {
    let mut out = Vec::new();
    for child in doc.children(id) {
        let tag = doc.tag(child);
        if is_hierarchy_tag(tag) {
            out.push(OutlineNode {
                eid: doc.eid(child).map(str::to_string),
                kind: tag.to_string(),
                num: doc.find_child(child, "num").map(|n| doc.text_of(n)),
                heading: doc.find_child(child, "heading").map(|h| doc.text_of(h)),
                children: outline_children(doc, child),
            });
        } else {
            // Nicht-Hierarchie-Zwischenknoten (content, blockList, …) transparent
            // durchlaufen — XSD-Lockerheit erlaubt sie überall (X17.8).
            out.extend(outline_children(doc, child));
        }
    }
    out
}

fn flatten_filter(tree: &[OutlineNode], kind: &str) -> Vec<OutlineNode> {
    let mut out = Vec::new();
    for node in tree {
        if node.kind == kind {
            out.push(OutlineNode {
                children: Vec::new(),
                ..node.clone()
            });
        }
        out.extend(flatten_filter(&node.children, kind));
    }
    out
}

/// Treffer eines eId-Lookups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EidHit {
    /// Der aufgelöste Knoten (erster Treffer in Dokumentreihenfolge).
    pub node: NodeId,
    /// Anzahl weiterer Knoten mit derselben eId — Eindeutigkeit ist
    /// NICHT garantiert (842 Corpus-Dateien, X15.3).
    pub duplicates: usize,
    /// `true`, wenn der Treffer erst über eId-Normalisierung gefunden wurde
    /// (`art_14a` ↔ `art_14_a`, X9.4).
    pub via_normalization: bool,
}

/// AKN-STR-02: Löst eine eId zum Element auf — erst exakt, dann über die
/// JOLux-Normalisierungsregel (X9.4). Duplikate werden ausgewiesen, nie
/// verschluckt.
pub fn resolve_eid(doc: &AknDocument, eid: &str) -> Result<EidHit, AknError> {
    let exact = doc.lookup_eid(eid);
    if let Some(&first) = exact.first() {
        return Ok(EidHit {
            node: first,
            duplicates: exact.len() - 1,
            via_normalization: false,
        });
    }
    // Normalisierter Vergleich in beide Richtungen — JOLux liefert `art_14a`,
    // das XML kann `art_14_a` tragen (und umgekehrt).
    let target = normalize_eid(eid);
    let mut hits: Vec<NodeId> = Vec::new();
    for (key, nodes) in doc.all_eids() {
        if normalize_eid(key) == target {
            hits.extend_from_slice(nodes);
        }
    }
    hits.sort_unstable();
    match hits.first() {
        Some(&first) => Ok(EidHit {
            node: first,
            duplicates: hits.len() - 1,
            via_normalization: true,
        }),
        None => Err(AknError::EidNotFound(eid.to_string())),
    }
}

/// Ein Schritt im Gliederungs-Pfad.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathStep {
    /// eId des Vorfahren.
    pub eid: Option<String>,
    /// Element-Typ.
    pub kind: String,
    /// Nummer aus `<num>`.
    pub num: Option<String>,
    /// Überschrift aus `<heading>`.
    pub heading: Option<String>,
}

/// AKN-STR-03: Gliederungs-Pfad von der Wurzel zum Element (inklusive),
/// beschränkt auf Hierarchie-Elemente — das Pflicht-Metadatum
/// `section_path` jedes Chunks (X14.3).
pub fn get_section_path(doc: &AknDocument, eid: &str) -> Result<Vec<PathStep>, AknError> {
    let hit = resolve_eid(doc, eid)?;
    Ok(section_path_of(doc, hit.node))
}

/// Pfad-Berechnung direkt über den Knoten (intern von TXT/CHK genutzt).
pub(crate) fn section_path_of(doc: &AknDocument, node: NodeId) -> Vec<PathStep> {
    let mut steps = Vec::new();
    let mut cur = Some(node);
    while let Some(c) = cur {
        if is_hierarchy_tag(doc.tag(c)) {
            steps.push(PathStep {
                eid: doc.eid(c).map(str::to_string),
                kind: doc.tag(c).to_string(),
                num: doc.find_child(c, "num").map(|n| doc.text_of(n)),
                heading: doc.find_child(c, "heading").map(|h| doc.text_of(h)),
            });
        }
        cur = doc.parent(c);
    }
    steps.reverse();
    steps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdoc::{sample, stichtag};

    #[test]
    fn outline_reflects_hierarchy() {
        let doc = sample();
        let resp = get_document_structure(&doc, None, stichtag()).unwrap();
        let tree = resp.data();
        let chap = tree.iter().find(|n| n.kind == "chapter").unwrap();
        assert_eq!(chap.eid.as_deref(), Some("chap_1"));
        assert_eq!(chap.children.iter().filter(|c| c.kind == "article").count(), 2);
        assert_eq!(resp.provenance().eli.as_str(), "eli/cc/2017/762");
    }

    #[test]
    fn type_filter_enumerates_articles_flat() {
        let doc = sample();
        let resp = get_document_structure(&doc, Some("article"), stichtag()).unwrap();
        let arts = resp.data();
        assert_eq!(arts.len(), 3);
        assert!(arts.iter().all(|a| a.children.is_empty()));
        assert_eq!(arts[0].eid.as_deref(), Some("art_1"));
    }

    #[test]
    fn resolve_eid_falls_back_to_normalization() {
        let doc = sample();
        // JOLux-Schreibweise `art_14a` muss das XML-Element `art_14_a` finden.
        let hit = resolve_eid(&doc, "art_14a").unwrap();
        assert!(hit.via_normalization);
        assert_eq!(doc.eid(hit.node), Some("art_14_a"));
        assert!(matches!(
            resolve_eid(&doc, "art_999"),
            Err(AknError::EidNotFound(_))
        ));
    }

    #[test]
    fn section_path_walks_root_to_element() {
        let doc = sample();
        let path = get_section_path(&doc, "art_1/para_1").unwrap();
        let kinds: Vec<&str> = path.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(kinds, ["chapter", "article", "paragraph"]);
        assert_eq!(path[0].eid.as_deref(), Some("chap_1"));
    }
}
