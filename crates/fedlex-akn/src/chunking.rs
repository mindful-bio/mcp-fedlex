//! Primitive: Hollowing & RAG-Chunking (Lexikon AKN-CHK-01/02, Rulebook X14/X20).

use crate::components::{get_component_document, list_components};
use crate::doc::{classify_pattern, get_frbr_metadata, DocPattern};
use crate::dom::{AknDocument, NodeId};
use crate::error::AknError;
use crate::structure::section_path_of;
use crate::text::has_eid_descendant;
use serde::{Deserialize, Serialize};

/// Ein Element der ausgehöhlten Dokument-Sicht.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HollowedElement {
    /// eId des Elements.
    pub eid: String,
    /// Element-Typ.
    pub kind: String,
    /// `true` = eId-Blatt (kein eId-Nachfahre) — trägt den echten Text.
    pub is_leaf: bool,
    /// Blatt: Normtext. Eltern-Container: Platzhalter mit den direkten
    /// eId-Kindern.
    pub text: String,
}

/// AKN-CHK-01: Höhlt das Dokument aus — nur eId-Blätter behalten ihren Text,
/// Eltern-Container werden zu Platzhaltern.
///
/// Eltern-Texte sind die Konkatenation ihrer Kinder (87.1 % Redundanz, X20.2).
/// Wer naiv alle eId-Elemente als Chunks indexiert, hat jeden Satz 3-4× im
/// Index. Beim Energiegesetz: 117'647 → ~15'156 Zeichen (X20.1).
pub fn hollow_document(doc: &AknDocument) -> Vec<HollowedElement> {
    let mut entries: Vec<(NodeId, &str)> = doc
        .all_eids()
        .flat_map(|(eid, nodes)| nodes.iter().map(move |&n| (n, eid)))
        .collect();
    entries.sort_unstable_by_key(|&(n, _)| n);
    entries
        .into_iter()
        .map(|(node, eid)| {
            let is_leaf = !has_eid_descendant(doc, node);
            let text = if is_leaf {
                doc.text_of(node)
            } else {
                let children = direct_eid_children(doc, node).join(", ");
                format!("[Siehe Unterelemente: {children}]")
            };
            HollowedElement {
                eid: eid.to_string(),
                kind: doc.tag(node).to_string(),
                is_leaf,
                text,
            }
        })
        .collect()
}

/// Nächste eId-tragende Nachfahren (BFS, stoppt an jedem eId-Knoten).
fn direct_eid_children(doc: &AknDocument, node: NodeId) -> Vec<String> {
    let mut out = Vec::new();
    let mut queue: Vec<NodeId> = doc.children(node).collect();
    let mut i = 0;
    while i < queue.len() {
        let c = queue[i];
        i += 1;
        match doc.eid(c) {
            Some(eid) => out.push(eid.to_string()),
            None => queue.extend(doc.children(c)),
        }
    }
    out
}

/// Die 8 Pflicht-Metadaten eines RAG-Chunks (X14.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkMetadata {
    /// SR-Nummer ohne Präfix (`730.0`).
    pub sr: Option<String>,
    /// Erlass-Titel.
    pub title: Option<String>,
    /// Work-URI (`/eli/cc/2017/762`).
    pub eli: Option<String>,
    /// Sprache der Expression.
    pub language: Option<String>,
    /// `jolux:dateDocument` aus dem FRBR-Block.
    pub date: Option<String>,
    /// Gliederungs-Pfad als eId-Liste.
    pub section_path: Vec<String>,
    /// eId der Chunk-Quelle.
    pub eid: Option<String>,
    /// Sammlung aus dem ELI-Pfad (`cc`, `oc`, `fga`).
    pub collection: Option<String>,
}

/// Ein RAG-Chunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chunk {
    /// Stabile ID: `{eli}#{eid}` bzw. `{eli}#idx{n}`.
    pub chunk_id: String,
    /// Chunk-Text (Nummer, Überschrift, Normtext).
    pub text: String,
    /// Die 8 Pflicht-Metadaten.
    pub metadata: ChunkMetadata,
}

/// Schwelle für Artikel-Splitting (Artikel-Median liegt bei ~550 Zeichen,
/// X14.2 — was darüber hinausschiesst, wird pro Absatz gesplittet).
const SPLIT_THRESHOLD: usize = 2000;

/// AKN-CHK-02: Zerlegt das Dokument musterabhängig in RAG-Chunks (X14.1).
///
/// STRUCTURED/FLAT_ARTICLES → pro Artikel (Übergrosse pro Absatz),
/// LEVEL_BASED → pro Level-Blatt, AMENDMENT → pro `<mod>`,
/// NO_BODY → pro Nicht-Stub-Component, OTHER → `<p>`-Gruppen.
pub fn chunk_document(doc: &AknDocument) -> Result<Vec<Chunk>, AknError> {
    let meta = get_frbr_metadata(doc)?;
    let info = classify_pattern(doc);
    let base = BaseMeta::from_frbr(&meta);
    let mut chunks = Vec::new();
    // Scope auf den Body — Components haben eigene FRBR-Werke (X19.8) und
    // werden im NO_BODY-Zweig bzw. via CMP-02 separat gechunkt.
    let body = doc
        .find_child(doc.root(), "body")
        .or_else(|| doc.find_child(doc.root(), "mainBody"))
        .unwrap_or_else(|| doc.root());

    match info.pattern {
        DocPattern::Structured | DocPattern::FlatArticles => {
            for art in doc.find_all(body, "article") {
                let text = doc.text_of(art);
                let paras: Vec<NodeId> = doc
                    .children(art)
                    .filter(|&c| doc.tag(c) == "paragraph")
                    .collect();
                // Split nur, wenn es auch Absätze gibt — ein übergrosser
                // Artikel ohne direkte paragraph-Kinder bleibt sonst EIN
                // Chunk statt stillschweigend zu verschwinden.
                if text.chars().count() > SPLIT_THRESHOLD && !paras.is_empty() {
                    for para in paras {
                        push_chunk(&mut chunks, doc, &base, Some(para), doc.text_of(para));
                    }
                } else {
                    push_chunk(&mut chunks, doc, &base, Some(art), text);
                }
            }
        }
        DocPattern::LevelBased => {
            for lvl in doc.find_all(body, "level") {
                // Nur Level-Blätter (kein Kind-Level) — Eltern sind redundant (X20.2).
                if doc.find_all(lvl, "level").len() == 1 {
                    push_chunk(&mut chunks, doc, &base, Some(lvl), doc.text_of(lvl));
                }
            }
        }
        DocPattern::Amendment => {
            for m in doc.find_all(body, "mod") {
                push_chunk(&mut chunks, doc, &base, Some(m), doc.text_of(m));
            }
        }
        DocPattern::NoBody => {
            for comp in list_components(doc) {
                if comp.is_empty_stub {
                    continue;
                }
                let inner = get_component_document(doc, comp.index)?;
                // Component rekursiv chunken — eigenes FRBR-Werk (X19.8).
                chunks.extend(chunk_document(&inner)?);
            }
        }
        DocPattern::Other => {
            let mut group = String::new();
            for p in doc.find_all(body, "p") {
                let t = doc.text_of(p);
                if !group.is_empty() && group.chars().count() + t.chars().count() > SPLIT_THRESHOLD
                {
                    push_chunk(&mut chunks, doc, &base, None, std::mem::take(&mut group));
                }
                if !t.is_empty() {
                    if !group.is_empty() {
                        group.push('\n');
                    }
                    group.push_str(&t);
                }
            }
            if !group.is_empty() {
                push_chunk(&mut chunks, doc, &base, None, group);
            }
        }
    }
    Ok(chunks)
}

struct BaseMeta {
    sr: Option<String>,
    title: Option<String>,
    eli: Option<String>,
    language: Option<String>,
    date: Option<String>,
    collection: Option<String>,
}

impl BaseMeta {
    fn from_frbr(m: &crate::doc::FrbrMetadata) -> Self {
        // Datierte Konsolidierungs-URI auf Work-Ebene reduzieren — Chunk-IDs
        // müssen über Fassungen stabil und mit JOLux joinbar sein.
        let work = crate::doc::work_eli_path(&m.eli_work).map(str::to_string);
        let collection = work
            .as_deref()
            .and_then(|p| p.strip_prefix("eli/"))
            .and_then(|rest| rest.split('/').next())
            .map(str::to_string);
        let date = m
            .dates
            .iter()
            .find(|(n, _)| n == "jolux:dateDocument")
            .or_else(|| m.dates.first())
            .map(|(_, d)| d.clone());
        BaseMeta {
            sr: m
                .sr_number
                .as_deref()
                .map(|s| s.trim_start_matches("SR ").to_string()),
            title: m.title.clone(),
            eli: work.or_else(|| Some(m.eli_work.clone())),
            language: m.language.clone(),
            date,
            collection,
        }
    }
}

fn push_chunk(
    chunks: &mut Vec<Chunk>,
    doc: &AknDocument,
    base: &BaseMeta,
    node: Option<NodeId>,
    text: String,
) {
    if text.is_empty() {
        return;
    }
    let eid = node.and_then(|n| doc.eid(n)).map(str::to_string);
    let section_path = node
        .map(|n| {
            section_path_of(doc, n)
                .into_iter()
                .filter_map(|s| s.eid)
                .collect()
        })
        .unwrap_or_default();
    let eli = base.eli.clone().unwrap_or_default();
    let chunk_id = match &eid {
        Some(e) => format!("{eli}#{e}"),
        None => format!("{eli}#idx{}", chunks.len()),
    };
    chunks.push(Chunk {
        chunk_id,
        text,
        metadata: ChunkMetadata {
            sr: base.sr.clone(),
            title: base.title.clone(),
            eli: base.eli.clone(),
            language: base.language.clone(),
            date: base.date.clone(),
            section_path,
            eid,
            collection: base.collection.clone(),
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdoc::sample;

    #[test]
    fn hollowing_keeps_leaf_text_and_hollows_parents() {
        let doc = sample();
        let hollowed = hollow_document(&doc);
        let by_eid = |eid: &str| hollowed.iter().find(|h| h.eid == eid).unwrap();

        let art1 = by_eid("art_1");
        assert!(!art1.is_leaf);
        assert!(art1.text.starts_with("[Siehe Unterelemente:"));
        assert!(art1.text.contains("art_1/para_1"));
        assert!(art1.text.contains("art_1/para_2"));

        let para = by_eid("art_1/para_1");
        assert!(para.is_leaf);
        assert!(para.text.contains("Energieversorgung"));

        // Redundanz-Check (X20.2): Eltern tragen keinen eigenen Normtext mehr.
        let leaf_chars: usize = hollowed
            .iter()
            .filter(|h| h.is_leaf)
            .map(|h| h.text.chars().count())
            .sum();
        assert!(leaf_chars > 0);
    }

    #[test]
    fn chunks_structured_doc_per_article() {
        let doc = sample();
        let chunks = chunk_document(&doc).unwrap();
        // 3 Artikel, alle unter der Split-Schwelle.
        assert_eq!(chunks.len(), 3);
        let c = &chunks[0];
        assert_eq!(c.chunk_id, "eli/cc/2017/762#art_1");
        assert!(c.text.contains("Energieversorgung"));
        let m = &c.metadata;
        assert_eq!(m.sr.as_deref(), Some("730.0"));
        assert_eq!(m.title.as_deref(), Some("Energiegesetz"));
        assert_eq!(m.collection.as_deref(), Some("cc"));
        assert_eq!(m.language.as_deref(), Some("de"));
        assert_eq!(m.date.as_deref(), Some("2018-01-01"));
        assert_eq!(m.section_path, ["chap_1", "art_1"]);
        assert_eq!(m.eid.as_deref(), Some("art_1"));
    }

    #[test]
    fn oversized_article_without_paragraphs_is_not_lost() {
        // Regressionstest: übergrosser Artikel ohne direkte paragraph-Kinder
        // muss als EIN Chunk erhalten bleiben, nicht stillschweigend wegfallen.
        let long = "Sehr langer Normtext. ".repeat(150); // > 2000 Zeichen
        let xml = format!(
            r#"<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0">
              <act name="publicLaw"><meta><identification>
                <FRBRWork><FRBRuri value="/eli/cc/2000/1"/></FRBRWork>
                <FRBRExpression><FRBRlanguage language="de"/></FRBRExpression>
              </identification></meta>
              <body><article eId="art_1"><num>Art. 1</num>
                <content><p>{long}</p></content>
              </article></body></act></akomaNtoso>"#
        );
        let doc = AknDocument::parse(&xml).unwrap();
        let chunks = chunk_document(&doc).unwrap();
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].text.chars().count() > 2000);
        assert_eq!(chunks[0].metadata.eid.as_deref(), Some("art_1"));
    }

    #[test]
    fn chunk_ids_use_undated_work_eli() {
        // Datierte Konsolidierungs-URI (Live-Form) darf NICHT in die Chunk-ID.
        let xml = r#"<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0">
          <act name="publicLaw"><meta><identification>
            <FRBRWork><FRBRuri value="https://fedlex.data.admin.ch/eli/cc/2017/762/20260401"/></FRBRWork>
            <FRBRExpression><FRBRlanguage language="de"/></FRBRExpression>
          </identification></meta>
          <body><article eId="art_1"><num>Art. 1</num>
            <content><p>Kurzer Text.</p></content>
          </article></body></act></akomaNtoso>"#;
        let doc = AknDocument::parse(xml).unwrap();
        let chunks = chunk_document(&doc).unwrap();
        assert_eq!(chunks[0].chunk_id, "eli/cc/2017/762#art_1");
        assert_eq!(chunks[0].metadata.eli.as_deref(), Some("eli/cc/2017/762"));
        assert_eq!(chunks[0].metadata.collection.as_deref(), Some("cc"));
    }
}
