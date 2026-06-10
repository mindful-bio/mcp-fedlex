//! # fedlex-akn — AKN-4.0-Lexikon als Funktionen
//!
//! Implementiert die 20 Primitive aus `docs/11_LEXICON_akn.md`. Jede
//! Funktion kapselt empirisch verifizierte Corpus-Regeln (Rulebook
//! `11_DATA_RULES_akn.md`, X0-X20) — Aufrufer brauchen kein AKN-Wissen.
//!
//! Schichtung analog `fedlex-jolux`: diese Crate ist transportfrei und
//! arbeitet auf bereits beschafften XML-Bytes. Die Manifestation-URL liefert
//! die JOLux-Brücke (JLX-RES-04), den Stichtag der Aufrufer.
//!
//! Rechtsaussagen (Normtext, Struktur, Änderungen, Verweise) kommen als
//! `Response<T>` mit Provenance (ADR-004), Helfer und Klassifikationen als
//! nackte Werte.

#![forbid(unsafe_code)]

mod chunking;
mod components;
mod doc;
mod dom;
mod error;
mod markdown;
mod modifications;
mod references;
mod special;
mod structure;
mod text;

pub use chunking::{chunk_document, hollow_document, Chunk, ChunkMetadata, HollowedElement};
pub use components::{get_component_document, list_components, ComponentInfo};
pub use doc::{classify_pattern, get_frbr_metadata, DocPattern, FrbrMetadata, PatternInfo};
pub use dom::{
    is_hierarchy_tag, normalize_eid, normalize_text, AknDocument, Content, NodeId,
    HIERARCHY_TAGS,
};
pub use error::AknError;
pub use markdown::get_readable_document;
pub use modifications::{extract_change_notes, get_modifications, ChangeNote, Modification};
pub use references::{get_all_references, parse_unlinked_ref, ParsedRef, RefKind, Reference};
pub use special::{detect_foreign_content, extract_tables, ForeignContent, ForeignKind, TableInfo};
pub use structure::{
    get_document_structure, get_section_path, resolve_eid, EidHit, OutlineNode, PathStep,
};
pub use text::{
    get_article_text, get_element_text, search_text, ElementText, Note, RefTarget, SearchHit,
};

/// Geteilte Test-Fixture — ein EnG-artiges Mini-Dokument, das alle
/// Corpus-Eigenheiten abdeckt (Fussnote im Wortinneren, href-lose refs,
/// Alt-Notation-eId, mod+quotedStructure, Component mit eigenem FRBR,
/// MathML ohne Namespace, Tabelle).
#[cfg(test)]
pub(crate) mod testdoc {
    use crate::dom::AknDocument;
    use fedlex_core::ValidAsOf;
    use time::macros::date;

    /// Stichtag für alle Tests.
    pub fn stichtag() -> ValidAsOf {
        ValidAsOf::new(date!(2026 - 06 - 01))
    }

    /// Geparste Fixture.
    pub fn sample() -> AknDocument {
        AknDocument::parse(SAMPLE_ACT).expect("Fixture muss parsen")
    }

    pub const SAMPLE_ACT: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<akomaNtoso xmlns="http://docs.oasis-open.org/legaldocml/ns/akn/3.0">
  <act name="publicLaw">
    <meta>
      <identification source="#ch.bk">
        <FRBRWork>
          <FRBRthis value="/eli/cc/2017/762"/>
          <FRBRuri value="/eli/cc/2017/762"/>
          <FRBRdate date="2018-01-01" name="jolux:dateDocument"/>
          <FRBRauthor href="#ch.bk"/>
          <FRBRcountry value="ch"/>
          <FRBRnumber value="SR 730.0"/>
          <FRBRname xml:lang="de" value="Energiegesetz"/>
        </FRBRWork>
        <FRBRExpression>
          <FRBRthis value="/eli/cc/2017/762/de"/>
          <FRBRuri value="/eli/cc/2017/762/de"/>
          <FRBRlanguage language="de"/>
        </FRBRExpression>
        <FRBRManifestation>
          <FRBRthis value="/eli/cc/2017/762/de/xml"/>
          <FRBRformat value="xml"/>
        </FRBRManifestation>
      </identification>
      <references source="#ch.bk">
        <TLCOrganization eId="ch.bk" href="https://www.bk.admin.ch/" showAs="Bundeskanzlei"/>
      </references>
    </meta>
    <preface>
      <p><docNumber>SR 730.0</docNumber> <docTitle>Energiegesetz (EnG)</docTitle></p>
    </preface>
    <preamble>
      <p>Die Bundesversammlung der Schweizerischen Eidgenossenschaft,
      gestützt auf <ref href="https://fedlex.data.admin.ch/eli/cc/1999/404">Art. 89 BV</ref><authorialNote marker="1">SR <ref href="https://fedlex.data.admin.ch/eli/cc/1999/404">101</ref></authorialNote>,
      beschliesst:</p>
    </preamble>
    <body>
      <chapter eId="chap_1">
        <num>1. Kapitel:</num>
        <heading>Allgemeine Bestimmungen</heading>
        <article eId="art_1">
          <num>Art. 1</num>
          <heading>Zweck</heading>
          <paragraph eId="art_1/para_1">
            <num>1</num>
            <content><p>Dieses Gesetz soll zu einer ausreichenden Energieversorgung beitragen.</p></content>
          </paragraph>
          <paragraph eId="art_1/para_2">
            <num>2</num>
            <content><p>Es bezweckt den sparsamen Energie<authorialNote marker="2">Fassung gemäss Ziff. I des BG vom 21. Juni 2019 (<ref href="https://fedlex.data.admin.ch/eli/oc/2020/752">AS 2020 752</ref>).</authorialNote>verbrauch.</p></content>
          </paragraph>
        </article>
        <article eId="art_2">
          <num>Art. 2</num>
          <heading>Richtwerte</heading>
          <paragraph eId="art_2/para_1">
            <num>1</num>
            <content>
              <blockList eId="art_2/para_1/list_1">
                <listIntroduction>Es gelten:</listIntroduction>
                <item eId="art_2/para_1/list_1/item_a"><num>a.</num><p>Wasserkraft gemäss <ref>Art. 1</ref>;</p></item>
                <item eId="art_2/para_1/list_1/item_b"><num>b.</num><p>SR-Verweis <ref>101</ref>.</p></item>
              </blockList>
              <table eId="art_2/para_1/tbl_1">
                <tr><th><p>Jahr</p></th><th><p>GWh</p></th></tr>
                <tr><td><p>2035</p></td><td><p>37400</p></td></tr>
              </table>
              <foreign><mrow><mi>P</mi><mo>=</mo><mn>2</mn></mrow></foreign>
            </content>
          </paragraph>
        </article>
      </chapter>
      <article eId="art_14_a">
        <num>Art. 14a</num>
        <heading>Alt-Notation</heading>
        <paragraph eId="art_14_a/para_1">
          <content><p>Buchstaben-Suffix-Test für die eId-Normalisierung.</p></content>
        </paragraph>
      </article>
      <level eId="lvl_u1">
        <num>II</num>
        <content><p>Levelinhalt für das OC-Muster.</p></content>
      </level>
      <mod eId="mod_1">
        <quotedStructure eId="mod_1/quot_1">
          <paragraph eId="mod_1/quot_1/para_3">
            <num>3</num>
            <content><p>Neuer Wortlaut des Absatzes.</p></content>
          </paragraph>
        </quotedStructure>
      </mod>
      <signature eId="sig_1"><p>Im Namen des Schweizerischen Nationalrates</p></signature>
    </body>
    <components>
      <component eId="cmp_1">
        <doc name="annex">
          <meta>
            <identification source="#ch.bk">
              <FRBRWork>
                <FRBRthis value="/eli/cc/2017/762/anx_1"/>
                <FRBRuri value="/eli/cc/2017/762/anx_1"/>
                <FRBRdate date="2018-01-01" name="jolux:dateDocument"/>
                <FRBRauthor href="#ch.bk"/>
                <FRBRname value="Anhang 1"/>
              </FRBRWork>
              <FRBRExpression>
                <FRBRthis value="/eli/cc/2017/762/anx_1/de"/>
                <FRBRlanguage language="de"/>
              </FRBRExpression>
              <FRBRManifestation>
                <FRBRthis value="/eli/cc/2017/762/anx_1/de/xml"/>
                <FRBRformat value="xml"/>
              </FRBRManifestation>
            </identification>
          </meta>
          <mainBody>
            <level eId="annex_1/lvl_1">
              <content><p>Inhalt des Anhangs mit genug Zeichen, um kein Leer-Stub zu sein. Aufzählung der internationalen Übereinkommen und weitere Details zur Energiestatistik.</p></content>
            </level>
          </mainBody>
        </doc>
      </component>
    </components>
  </act>
</akomaNtoso>"##;
}
