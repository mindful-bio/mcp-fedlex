//! fedlex-jolux - getestete, komponierbare JOLux-Primitive (SPARQL).
//!
//! Diese Crate ist der **Primitive-Katalog** der JOLux-Seite (Metadaten/Graph).
//! Jede Funktion ist ein kleines, einzeln getestetes Werkzeug über dem
//! [`SparqlClient`]-Trait. Höhere, agentenseitige Tools (in `mcp-reader`) werden
//! als **Kompositionen** dieser Primitive gebaut, nicht als Monolithen.
//!
//! Konvention (zwei Sorten):
//! - **Rechtsaussagen** (eine konkrete Norm/Fassung) liefern [`Response<T>`] mit
//!   Pflicht-Provenance (ELI + Stichtag + Systemzeit), ADR-004.
//! - **Reine Helfer** (Vocabulary-Label, URL-Auflösung) liefern nackte Werte,
//!   da sie keine Rechtsaussage sind.
//!
//! Grundlage der Regeln: `analyse-fedlex/10_DATA_RULES_jolux.md` (Rulebook J0–J20),
//! das die offizielle JOLux-Ontologie-Doku konsolidiert.
//!
//! [`Response<T>`]: fedlex_core::Response

#![forbid(unsafe_code)]

pub mod citations;
pub mod client;
pub mod error;
pub mod genesis;
pub mod impacts;
pub mod metadata;
pub mod publication;
pub mod resolve;
pub mod search;
pub mod subdivisions;
pub mod taxonomy;
pub mod temporal;
pub mod treaties;
pub mod vocabulary;

pub use citations::{Citation, CitationDirection, get_citations};
pub use client::{Language, MockSparqlClient, PREFIXES, SparqlClient, SparqlResults, val};
pub use error::JoluxError;
pub use genesis::{
    Consultation, ConsultationDocument, Draft, get_consultation_documents, get_consultations,
    get_drafts,
};
pub use impacts::{
    Impact, OutgoingImpact, get_article_history, get_impacts, get_outgoing_impacts, normalize_eid,
};
pub use metadata::{LawMetadata, get_law_metadata};
pub use publication::{
    FgaDocument, MemorialInfo, OcAct, get_fga_documents, get_memorial, get_oc_act,
};
pub use resolve::{
    Manifestation, ManifestationFormat, SrHit, list_expressions, resolve_manifestation,
    resolve_sr_number,
};
pub use search::{LawHit, search_law};
pub use subdivisions::{ANNEX_TYPE_URI, Subdivision, get_subdivisions, list_annexes};
pub use taxonomy::{RelatedLaw, TaxonomyEntry, find_related_by_topic, get_taxonomy};
pub use temporal::{
    Consolidation, InForce, Version, check_in_force, list_versions, resolve_consolidation_at,
};
pub use treaties::{TreatyHit, TreatyInfo, find_treaties, get_treaty_info};
pub use vocabulary::{
    NodeEdge, NodeNeighborhood, VOCABULARY_BASE, VocabularyConcept, explore_node, list_vocabulary,
    resolve_vocabulary_label,
};

/// Basis-URI von Fedlex. ELI-Werte werden relativ dazu gespeichert (`eli/cc/...`).
pub const FEDLEX_BASE: &str = "https://fedlex.data.admin.ch/";

/// Expandiert einen [`Eli`](fedlex_core::Eli) zur vollen Fedlex-URI für SPARQL.
///
/// `eli/cc/2017/762` -> `https://fedlex.data.admin.ch/eli/cc/2017/762`.
pub fn eli_uri(eli: &fedlex_core::Eli) -> String {
    format!("{FEDLEX_BASE}{}", eli.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fedlex_core::Eli;

    #[test]
    fn eli_uri_expands_to_full_fedlex_uri() {
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        assert_eq!(
            eli_uri(&eli),
            "https://fedlex.data.admin.ch/eli/cc/2017/762"
        );
    }
}
