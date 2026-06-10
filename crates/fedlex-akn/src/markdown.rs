//! Primitiv: Lesbares Gesamtdokument als Markdown (Lexikon AKN-TXT-03).
//!
//! Hierarchie → Überschriften, Blätter → Absätze, Tabellen → Zeilen.
//! Für Anzeige und LLM-Kontext, nicht für Zitate (dafür TXT-01/02).

use crate::doc::{get_frbr_metadata, provenance};
use crate::dom::{is_hierarchy_tag, AknDocument, NodeId};
use crate::error::AknError;
use fedlex_core::{Response, ValidAsOf};

/// AKN-TXT-03: Rendert das ganze Dokument als Markdown.
///
/// Titel aus `FRBRname`, SR-Nummer aus `FRBRnumber`, dann Präambel und Body.
/// Fussnoten und `<foreign>`-Inseln sind ausgeschlossen — wer sie braucht,
/// nutzt MOD-02 bzw. SPC-02. Beim Energiegesetz schrumpft so 1.18 MB XML
/// auf ~15-20 k lesbare Zeichen (X20.1-Grössenordnung).
pub fn get_readable_document(
    doc: &AknDocument,
    as_of: ValidAsOf,
) -> Result<Response<String>, AknError> {
    let prov = provenance(doc, as_of)?;
    let meta = get_frbr_metadata(doc)?;
    let mut out = String::new();

    if let Some(title) = &meta.title {
        out.push_str(&format!("# {title}\n\n"));
    }
    if let Some(sr) = &meta.sr_number {
        out.push_str(&format!("**{sr}**\n\n"));
    }
    if let Some(preamble) = doc.find_child(doc.root(), "preamble") {
        let text = doc.text_of(preamble);
        if !text.is_empty() {
            out.push_str(&text);
            out.push_str("\n\n");
        }
    }
    let body = doc
        .find_child(doc.root(), "body")
        .or_else(|| doc.find_child(doc.root(), "mainBody"));
    if let Some(body) = body {
        render(doc, body, 1, &mut out);
    }
    Ok(Response::new(out.trim_end().to_string(), prov))
}

fn render(doc: &AknDocument, id: NodeId, depth: usize, out: &mut String) {
    for child in doc.children(id) {
        let tag = doc.tag(child);
        if is_hierarchy_tag(tag) {
            let num = doc.find_child(child, "num").map(|n| doc.text_of(n));
            let heading = doc.find_child(child, "heading").map(|h| doc.text_of(h));
            let label = [num, heading]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            if has_hierarchy_children(doc, child) {
                if !label.is_empty() {
                    let hashes = "#".repeat((depth + 1).min(6));
                    out.push_str(&format!("{hashes} {label}\n\n"));
                }
                render(doc, child, depth + 1, out);
            } else {
                // Blatt: Text en bloc (enthält num/heading bereits via text_of).
                let text = doc.text_of(child);
                if !text.is_empty() {
                    out.push_str(&text);
                    out.push_str("\n\n");
                }
            }
        } else if tag == "authorialNote" || tag == "foreign" {
            continue;
        } else {
            render(doc, child, depth, out);
        }
    }
}

fn has_hierarchy_children(doc: &AknDocument, id: NodeId) -> bool {
    doc.descendants(id)
        .into_iter()
        .skip(1)
        .any(|d| is_hierarchy_tag(doc.tag(d)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdoc::{sample, stichtag};

    #[test]
    fn renders_title_headings_and_leaf_text() {
        let doc = sample();
        let md = get_readable_document(&doc, stichtag()).unwrap();
        let md = md.data();
        assert!(md.starts_with("# Energiegesetz"), "got: {md}");
        assert!(md.contains("**SR 730.0**"));
        assert!(md.contains("## 1. Kapitel: Allgemeine Bestimmungen"));
        assert!(md.contains("Art. 1"));
        assert!(md.contains("Energieversorgung"));
        // Fussnoten bleiben draussen.
        assert!(!md.contains("AS 2020 752"));
    }
}
