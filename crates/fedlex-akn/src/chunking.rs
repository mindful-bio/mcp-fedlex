//! Primitive: Hollowing & RAG-Chunking (Lexikon AKN-CHK-01/02, Rulebook X14/X20).

use crate::components::{get_component_document, list_components};
use crate::doc::{DocPattern, classify_pattern, get_frbr_metadata};
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
                        push_chunks_table_aware(&mut chunks, doc, &base, para);
                    }
                } else {
                    push_chunks_table_aware(&mut chunks, doc, &base, art);
                }
            }
        }
        DocPattern::LevelBased => {
            for lvl in doc.find_all(body, "level") {
                // Nur Level-Blätter (kein Kind-Level) — Eltern sind redundant (X20.2).
                if doc.find_all(lvl, "level").len() == 1 {
                    push_chunks_table_aware(&mut chunks, doc, &base, lvl);
                }
            }
        }
        DocPattern::Amendment => {
            for m in doc.find_all(body, "mod") {
                push_chunks_table_aware(&mut chunks, doc, &base, m);
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
    push_chunk_suffixed(chunks, doc, base, node, None, text);
}

/// Chunkt einen Knoten tabellen-bewusst (X13.3).
///
/// Sprengt der Knotentext die Split-Schwelle und enthält er Tabellen, wird
/// der Fliesstext ohne Tabellen EIN Chunk und jede Tabelle ein eigener.
/// Übergrosse Tabellen werden zeilengruppenweise gesplittet, die Kopfzeile
/// wird jeder Gruppe vorangestellt, damit die Spalten lesbar bleiben.
/// Tabellen sind semantische Einheiten und werden nie mitten in der Zeile
/// getrennt (X13.2).
fn push_chunks_table_aware(
    chunks: &mut Vec<Chunk>,
    doc: &AknDocument,
    base: &BaseMeta,
    node: NodeId,
) {
    let text = doc.text_of(node);
    let tables = doc.find_all(node, "table");
    if text.chars().count() <= SPLIT_THRESHOLD || tables.is_empty() {
        push_chunk(chunks, doc, base, Some(node), text);
        return;
    }

    // Fliesstext ohne Tabellen behält die Stamm-Chunk-ID des Knotens.
    push_chunk(chunks, doc, base, Some(node), doc.text_without_tables(node));

    for (t_idx, &table) in tables.iter().enumerate() {
        let suffix = format!("tbl{}", t_idx + 1);
        let table_text = doc.text_of(table);
        if table_text.chars().count() <= SPLIT_THRESHOLD {
            push_chunk_suffixed(chunks, doc, base, Some(node), Some(&suffix), table_text);
            continue;
        }

        // Übergross. Zeilengruppen bilden, Kopfzeile wiederholen.
        let trs = doc.find_all(table, "tr");
        let header = trs
            .first()
            .filter(|&&tr| doc.children(tr).any(|c| doc.tag(c) == "th"))
            .map(|&tr| doc.text_of(tr));
        let data_rows: Vec<String> = trs
            .iter()
            .skip(usize::from(header.is_some()))
            .map(|&tr| doc.text_of(tr))
            .filter(|t| !t.is_empty())
            .collect();

        let header_len = header.as_ref().map_or(0, |h| h.chars().count() + 1);
        let mut group = String::new();
        let mut part = 1usize;
        for row in data_rows {
            let would_be = header_len + group.chars().count() + row.chars().count() + 1;
            if !group.is_empty() && would_be > SPLIT_THRESHOLD {
                flush_table_group(
                    chunks,
                    doc,
                    base,
                    node,
                    &suffix,
                    &mut part,
                    header.as_deref(),
                    std::mem::take(&mut group),
                );
            }
            if !group.is_empty() {
                group.push('\n');
            }
            group.push_str(&row);
        }
        if !group.is_empty() {
            flush_table_group(
                chunks,
                doc,
                base,
                node,
                &suffix,
                &mut part,
                header.as_deref(),
                group,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn flush_table_group(
    chunks: &mut Vec<Chunk>,
    doc: &AknDocument,
    base: &BaseMeta,
    node: NodeId,
    suffix: &str,
    part: &mut usize,
    header: Option<&str>,
    group: String,
) {
    let text = match header {
        Some(h) => format!("{h}\n{group}"),
        None => group,
    };
    let part_suffix = format!("{suffix}/part{part}");
    push_chunk_suffixed(chunks, doc, base, Some(node), Some(&part_suffix), text);
    *part += 1;
}

fn push_chunk_suffixed(
    chunks: &mut Vec<Chunk>,
    doc: &AknDocument,
    base: &BaseMeta,
    node: Option<NodeId>,
    suffix: Option<&str>,
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
    let stem = match &eid {
        Some(e) => format!("{eli}#{e}"),
        None => format!("{eli}#idx{}", chunks.len()),
    };
    let chunk_id = match suffix {
        Some(s) => format!("{stem}/{s}"),
        None => stem,
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

    /// Baut einen Artikel mit Prosa und einer Tabelle mit `rows` Datenzeilen.
    fn doc_with_table(rows: usize) -> AknDocument {
        let mut table = String::from("<tr><th>Stoff</th><th>Grenzwert</th></tr>");
        for i in 0..rows {
            table.push_str(&format!(
                "<tr><td>Stoff Nummer {i} mit einem laengeren Namen</td><td>Grenzwert {i} \
                 Milligramm pro Kubikmeter Abluft</td></tr>"
            ));
        }
        let xml = format!(
            r#"<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0">
              <act name="publicLaw"><meta><identification>
                <FRBRWork><FRBRuri value="/eli/cc/2000/1"/></FRBRWork>
                <FRBRExpression><FRBRlanguage language="de"/></FRBRExpression>
              </identification></meta>
              <body><article eId="art_1"><num>Art. 1</num>
                <content><p>Die Grenzwerte richten sich nach folgender Tabelle.</p>
                <table eId="art_1/tbl_1">{table}</table></content>
              </article></body></act></akomaNtoso>"#
        );
        AknDocument::parse(&xml).unwrap()
    }

    #[test]
    fn small_table_stays_inside_article_chunk() {
        let chunks = chunk_document(&doc_with_table(3)).unwrap();
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].text.contains("Stoff Nummer 2"));
    }

    #[test]
    fn oversized_table_is_split_into_row_groups_with_repeated_header() {
        let chunks = chunk_document(&doc_with_table(120)).unwrap();
        // Prosa-Chunk plus mehrere Tabellen-Teile.
        assert!(
            chunks.len() > 2,
            "erwartet Prosa + Teile, war {}",
            chunks.len()
        );

        let prose = &chunks[0];
        assert_eq!(prose.chunk_id, "eli/cc/2000/1#art_1");
        assert!(prose.text.contains("Grenzwerte richten sich"));
        assert!(
            !prose.text.contains("Stoff Nummer"),
            "Tabelle gehoert nicht in den Prosa-Chunk"
        );

        // Jeder Teil traegt die wiederholte Kopfzeile und bleibt unter der Schwelle.
        let parts: Vec<&Chunk> = chunks[1..].iter().collect();
        for (i, part) in parts.iter().enumerate() {
            assert_eq!(
                part.chunk_id,
                format!("eli/cc/2000/1#art_1/tbl1/part{}", i + 1)
            );
            assert!(part.text.starts_with("Stoff Grenzwert"));
            assert!(part.text.chars().count() <= SPLIT_THRESHOLD + 100);
            assert_eq!(part.metadata.eid.as_deref(), Some("art_1"));
        }

        // Keine Zeile geht verloren.
        let all: String = parts.iter().map(|c| c.text.as_str()).collect();
        for i in [0usize, 59, 119] {
            assert!(
                all.contains(&format!("Stoff Nummer {i}")),
                "Zeile {i} fehlt"
            );
        }
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
