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

pub mod client;
pub mod error;
pub mod impacts;
pub mod metadata;
pub mod search;
pub mod temporal;
pub mod vocabulary;

pub use client::{val, Language, MockSparqlClient, SparqlClient, SparqlResults, PREFIXES};
pub use error::JoluxError;
pub use impacts::{get_impacts, Impact};
pub use metadata::{get_law_metadata, LawMetadata};
pub use search::{search_law, LawHit};
pub use temporal::{resolve_consolidation_at, Consolidation};
pub use vocabulary::resolve_vocabulary_label;

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
