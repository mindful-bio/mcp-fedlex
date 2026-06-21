//! Arena-DOM für AKN 3.0 — die Parse-Schicht des Lexikon-Primitivs AKN-DOC-01.
//!
//! Fedlex-XML ist **nicht strikt XSD-valid** (5 Violations, X17.1). Dieser
//! Parser ist deshalb bewusst tolerant: er kennt keine Schema-Regeln, nur den
//! `<akomaNtoso>`-Envelope. Text wird beim Einlesen normalisiert
//! (Soft-Hyphens raus, NBSP zu Leerzeichen, X18.3).

use crate::error::AknError;
use std::collections::HashMap;

/// Index eines Knotens in der Arena.
pub type NodeId = usize;

/// Die 11 real genutzten AKN-Hierarchie-Elemente (X17.3).
/// 17 weitere sind im Standard definiert, kommen im Corpus aber nie vor.
pub const HIERARCHY_TAGS: [&str; 11] = [
    "book",
    "part",
    "title",
    "chapter",
    "section",
    "subdivision",
    "article",
    "paragraph",
    "level",
    "transitional",
    "proviso",
];

/// Prüft, ob ein Tag ein Hierarchie-Element ist.
pub fn is_hierarchy_tag(tag: &str) -> bool {
    HIERARCHY_TAGS.contains(&tag)
}

/// Text-Normalisierung (Querschnitt-Invariante, X18.3).
/// Soft-Hyphens (`\u{ad}`, 3'131× im Corpus) entfernen,
/// NBSP (`\u{a0}`, 662'695×) zu Leerzeichen.
pub fn normalize_text(s: &str) -> String {
    s.replace('\u{00ad}', "").replace('\u{00a0}', " ")
}

/// eId-Normalisierung für den AKN↔JOLux-Abgleich (X9.4, J18.2).
/// Regel `_([a-z])($|/)` → `$1$2`, z.B. `art_14_a` → `art_14a`.
/// Die Regel ist die Brücke zwischen beiden Schichten und lebt in
/// `fedlex-core` ([`fedlex_core::normalize_eid`]) — hier nur re-exportiert.
pub use fedlex_core::normalize_eid;

/// Inhalt eines Knotens in Dokumentreihenfolge — Text und Kind-Elemente
/// bleiben verschränkt (wichtig für Fussnoten mitten im Satz, X6.4).
#[derive(Debug, Clone)]
pub enum Content {
    /// Kind-Element.
    Element(NodeId),
    /// Textstück (bereits normalisiert).
    Text(String),
}

#[derive(Debug, Clone)]
struct Node {
    tag: String,
    attrs: Vec<(String, String)>,
    parent: Option<NodeId>,
    content: Vec<Content>,
}

/// Ein geparstes AKN-Dokument (Arena-DOM, immutabel nach dem Parsen).
///
/// Wurzel ist das Dokument-Element — `<act>` auf Datei-Ebene oder `<doc>`
/// für ein per [`crate::components::get_component_document`] extrahiertes
/// Component-Dokument.
#[derive(Debug, Clone)]
pub struct AknDocument {
    nodes: Vec<Node>,
    root: NodeId,
    eid_index: HashMap<String, Vec<NodeId>>,
}

impl AknDocument {
    /// AKN-DOC-01 (Parse-Kern): Parst einen AKN-3.0-XML-String.
    ///
    /// Erwartet den `<akomaNtoso>`-Envelope mit genau einem Dokument-Element
    /// (im Corpus zu 100 % `<act name="publicLaw">`, X1.1). Die Beschaffung
    /// der XML-Bytes läuft über die JOLux-Brücke (JLX-RES-04 →
    /// Manifestation-URL) in der Transport-Schicht — diese Crate bleibt
    /// transportfrei, analog zum `SparqlClient`-Schnitt in `fedlex-jolux`.
    pub fn parse(xml: &str) -> Result<Self, AknError> {
        let opts = roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        };
        let tree = roxmltree::Document::parse_with_options(xml, opts)
            .map_err(|e| AknError::Parse(e.to_string()))?;
        let envelope = tree.root_element();
        if envelope.tag_name().name() != "akomaNtoso" {
            return Err(AknError::NotAkn(envelope.tag_name().name().to_string()));
        }
        let act = envelope
            .children()
            .find(|c| c.is_element())
            .ok_or(AknError::EmptyEnvelope)?;

        let mut doc = AknDocument {
            nodes: Vec::new(),
            root: 0,
            eid_index: HashMap::new(),
        };
        doc.root = doc.copy_rox(act, None);
        doc.build_eid_index();
        Ok(doc)
    }

    fn copy_rox(&mut self, n: roxmltree::Node<'_, '_>, parent: Option<NodeId>) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node {
            tag: n.tag_name().name().to_string(),
            attrs: n
                .attributes()
                .map(|a| (a.name().to_string(), a.value().to_string()))
                .collect(),
            parent,
            content: Vec::new(),
        });
        let mut content = Vec::new();
        for child in n.children() {
            if child.is_element() {
                content.push(Content::Element(self.copy_rox(child, Some(id))));
            } else if let Some(text) = child.text()
                && !text.is_empty()
            {
                content.push(Content::Text(normalize_text(text)));
            }
        }
        self.nodes[id].content = content;
        id
    }

    fn build_eid_index(&mut self) {
        let mut index: HashMap<String, Vec<NodeId>> = HashMap::new();
        for id in 0..self.nodes.len() {
            if let Some(eid) = self.attr(id, "eId") {
                index.entry(eid.to_string()).or_default().push(id);
            }
        }
        self.eid_index = index;
    }

    /// Wurzel-Element (`<act>` bzw. `<doc>`).
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Lokaler Tag-Name eines Knotens.
    pub fn tag(&self, id: NodeId) -> &str {
        &self.nodes[id].tag
    }

    /// Attributwert (lokaler Name, namespace-agnostisch — `xml:lang` als `lang`).
    pub fn attr(&self, id: NodeId, name: &str) -> Option<&str> {
        self.nodes[id]
            .attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// `@eId` des Knotens.
    pub fn eid(&self, id: NodeId) -> Option<&str> {
        self.attr(id, "eId")
    }

    /// Eltern-Knoten.
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.nodes[id].parent
    }

    /// Inhalt (Text + Elemente verschränkt, Dokumentreihenfolge).
    pub fn content(&self, id: NodeId) -> &[Content] {
        &self.nodes[id].content
    }

    /// Direkte Kind-Elemente.
    pub fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes[id].content.iter().filter_map(|c| match c {
            Content::Element(e) => Some(*e),
            Content::Text(_) => None,
        })
    }

    /// Erstes direktes Kind-Element mit dem gegebenen Tag.
    pub fn find_child(&self, id: NodeId, tag: &str) -> Option<NodeId> {
        self.children(id).find(|&c| self.tag(c) == tag)
    }

    /// Alle Elemente im Teilbaum (Preorder, inkl. Wurzel) mit dem Tag.
    pub fn find_all(&self, root: NodeId, tag: &str) -> Vec<NodeId> {
        let mut out = Vec::new();
        self.walk(root, &mut |id| {
            if self.tag(id) == tag {
                out.push(id);
            }
        });
        out
    }

    /// Alle Elemente im Teilbaum (Preorder, inkl. Wurzel).
    pub fn descendants(&self, root: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        self.walk(root, &mut |id| out.push(id));
        out
    }

    fn walk(&self, id: NodeId, f: &mut impl FnMut(NodeId)) {
        f(id);
        let children: Vec<NodeId> = self.children(id).collect();
        for c in children {
            self.walk(c, f);
        }
    }

    /// eId-Lookup (exakt). Liefert **alle** Treffer — Eindeutigkeit ist
    /// NICHT garantiert (842 Corpus-Dateien mit Duplikaten, X15.3).
    pub fn lookup_eid(&self, eid: &str) -> &[NodeId] {
        self.eid_index.get(eid).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Alle indexierten eIds (für Audits und Hollowing).
    pub fn all_eids(&self) -> impl Iterator<Item = (&str, &[NodeId])> {
        self.eid_index
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_slice()))
    }

    /// Nächste eId auf dem Selbst-oder-Vorfahren-Pfad.
    pub fn nearest_eid(&self, id: NodeId) -> Option<&str> {
        let mut cur = Some(id);
        while let Some(c) = cur {
            if let Some(eid) = self.eid(c) {
                return Some(eid);
            }
            cur = self.parent(c);
        }
        None
    }

    /// Extrahiert den Fliesstext eines Teilbaums.
    ///
    /// Querschnitt-Invarianten eingebaut: `<authorialNote>` (Änderungshistorie,
    /// kein Normtext, X6.4) und `<foreign>` (SVG/MathML, X18.4) sind vom
    /// Textfluss ausgeschlossen. Inline-Markup wird geflattet (X18.5).
    pub fn text_of(&self, id: NodeId) -> String {
        let mut out = String::new();
        self.write_text(id, &mut out);
        tidy(&out)
    }

    /// Wie [`text_of`](Self::text_of), aber ohne `<table>`-Teilbäume.
    ///
    /// Für das Tabellen-Splitting beim Chunking (X13.3). Der Fliesstext und
    /// die Tabellen werden dort getrennt gechunkt, damit übergrosse Tabellen
    /// den Prosa-Chunk nicht sprengen.
    pub fn text_without_tables(&self, id: NodeId) -> String {
        let mut out = String::new();
        self.write_text_excluding(id, &mut out, "table");
        tidy(&out)
    }

    fn write_text_excluding(&self, id: NodeId, out: &mut String, excluded: &str) {
        for c in &self.nodes[id].content {
            match c {
                Content::Text(t) => out.push_str(t),
                Content::Element(e) => {
                    let tag = self.tag(*e);
                    if tag == "authorialNote" || tag == "foreign" || tag == excluded {
                        continue;
                    }
                    self.write_text_excluding(*e, out, excluded);
                    match tag {
                        "num" | "td" | "th" => out.push(' '),
                        "p" | "paragraph" | "heading" | "item" | "listIntroduction" | "intro"
                        | "tr" | "block" | "content" => out.push('\n'),
                        _ => {}
                    }
                }
            }
        }
    }

    fn write_text(&self, id: NodeId, out: &mut String) {
        for c in &self.nodes[id].content {
            match c {
                Content::Text(t) => out.push_str(t),
                Content::Element(e) => {
                    let tag = self.tag(*e);
                    if tag == "authorialNote" || tag == "foreign" {
                        continue;
                    }
                    self.write_text(*e, out);
                    match tag {
                        "num" | "td" | "th" => out.push(' '),
                        "p" | "paragraph" | "heading" | "item" | "listIntroduction" | "intro"
                        | "tr" | "block" | "content" => out.push('\n'),
                        _ => {}
                    }
                }
            }
        }
    }

    /// Wie [`text_of`](Self::text_of), aber inklusive Fussnotentext
    /// (für Anzeige-Zwecke, nie für Normtext-Zitate).
    pub fn text_with_notes(&self, id: NodeId) -> String {
        let mut out = String::new();
        self.write_text_with_notes(id, &mut out);
        tidy(&out)
    }

    fn write_text_with_notes(&self, id: NodeId, out: &mut String) {
        for c in &self.nodes[id].content {
            match c {
                Content::Text(t) => out.push_str(t),
                Content::Element(e) => {
                    if self.tag(*e) == "foreign" {
                        continue;
                    }
                    self.write_text_with_notes(*e, out);
                    if matches!(self.tag(*e), "p" | "paragraph" | "heading" | "item") {
                        out.push('\n');
                    }
                }
            }
        }
    }

    /// Kopiert einen Teilbaum in ein eigenständiges Dokument
    /// (Basis von AKN-CMP-02 — Components sind eigene FRBR-Werke, X19.8).
    pub fn subtree_document(&self, id: NodeId) -> AknDocument {
        let mut doc = AknDocument {
            nodes: Vec::new(),
            root: 0,
            eid_index: HashMap::new(),
        };
        doc.root = doc.copy_arena(self, id, None);
        doc.build_eid_index();
        doc
    }

    fn copy_arena(&mut self, src: &AknDocument, src_id: NodeId, parent: Option<NodeId>) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node {
            tag: src.nodes[src_id].tag.clone(),
            attrs: src.nodes[src_id].attrs.clone(),
            parent,
            content: Vec::new(),
        });
        let mut content = Vec::new();
        for c in &src.nodes[src_id].content {
            match c {
                Content::Text(t) => content.push(Content::Text(t.clone())),
                Content::Element(e) => {
                    content.push(Content::Element(self.copy_arena(src, *e, Some(id))));
                }
            }
        }
        self.nodes[id].content = content;
        id
    }
}

/// Zeilen trimmen, Leerzeilen entfernen — macht Blockgrenzen deterministisch.
fn tidy(s: &str) -> String {
    s.lines()
        .map(|l| l.trim().trim_end_matches(' '))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdoc::SAMPLE_ACT;

    #[test]
    fn parses_envelope_and_act() {
        let doc = AknDocument::parse(SAMPLE_ACT).unwrap();
        assert_eq!(doc.tag(doc.root()), "act");
        assert_eq!(doc.attr(doc.root(), "name"), Some("publicLaw"));
    }

    #[test]
    fn rejects_non_akn_root() {
        let err = AknDocument::parse("<html><body/></html>").unwrap_err();
        assert!(matches!(err, AknError::NotAkn(t) if t == "html"));
    }

    #[test]
    fn text_excludes_notes_and_foreign() {
        let doc = AknDocument::parse(SAMPLE_ACT).unwrap();
        let para = doc.lookup_eid("art_1/para_2")[0];
        let text = doc.text_of(para);
        // Fussnote mitten im Wort darf den Normtext nicht zerreissen (X6.4).
        assert!(text.contains("Energieverbrauch"), "got: {text}");
        assert!(!text.contains("AS 2020 752"), "Note leckt in Normtext");
    }

    #[test]
    fn normalizes_nbsp_and_soft_hyphen() {
        let xml = r#"<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0">
          <act name="publicLaw"><body><article eId="art_1">
            <content><p>Art.&#160;1 Ener&#173;gie</p></content>
          </article></body></act></akomaNtoso>"#;
        let doc = AknDocument::parse(xml).unwrap();
        let art = doc.lookup_eid("art_1")[0];
        assert_eq!(doc.text_of(art), "Art. 1 Energie");
    }

    #[test]
    fn eid_normalization_matches_jolux_rule() {
        assert_eq!(normalize_eid("art_14_a"), "art_14a");
        assert_eq!(normalize_eid("art_14_a/para_1"), "art_14a/para_1");
        assert_eq!(normalize_eid("art_14"), "art_14");
        assert_eq!(normalize_eid("chap_5_a"), "chap_5a");
    }

    #[test]
    fn duplicate_eids_are_collected_not_lost() {
        let xml = r#"<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0">
          <act name="publicLaw"><body>
            <level eId="lvl_u1"><content><p>eins</p></content></level>
            <level eId="lvl_u1"><content><p>zwei</p></content></level>
          </body></act></akomaNtoso>"#;
        let doc = AknDocument::parse(xml).unwrap();
        // X15.3: Duplikate abfangen, nicht verschlucken.
        assert_eq!(doc.lookup_eid("lvl_u1").len(), 2);
    }
}
