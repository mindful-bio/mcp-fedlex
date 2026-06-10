//! Fehlertypen der AKN-Schicht.

use fedlex_core::IdError;

/// Fehler beim Parsen oder Verarbeiten eines AKN-Dokuments.
#[derive(Debug, thiserror::Error)]
pub enum AknError {
    /// Das XML konnte nicht geparst werden (im Fedlex-Corpus nie beobachtet,
    /// 0 Parse-Fehler auf 15'807 Dateien, X15.1 — aber Eingaben sind Eingaben).
    #[error("XML-Parse-Fehler: {0}")]
    Parse(String),
    /// Die Wurzel ist kein `<akomaNtoso>`-Envelope.
    #[error("kein AKN-Dokument: Wurzel-Element ist <{0}>")]
    NotAkn(String),
    /// Der Envelope enthält kein Dokument-Element.
    #[error("leerer AKN-Envelope ohne <act>")]
    EmptyEnvelope,
    /// Kein `<identification>`-Block gefunden (in 100 % der Corpus-Dateien
    /// vorhanden, X2.2 — Fehlen deutet auf ein Fragment hin).
    #[error("kein FRBR-<identification>-Block im <meta>")]
    MissingFrbr,
    /// Die angefragte eId existiert nicht (auch nicht nach Normalisierung).
    #[error("eId `{0}` nicht im Dokument (auch normalisiert nicht)")]
    EidNotFound(String),
    /// Das Element trägt nicht den erwarteten Typ.
    #[error("eId `{eid}` ist <{found}>, erwartet <{expected}>")]
    WrongElementKind {
        /// Angefragte eId.
        eid: String,
        /// Tatsächlicher Tag.
        found: String,
        /// Erwarteter Tag.
        expected: &'static str,
    },
    /// Component-Index ausserhalb des Bereichs.
    #[error("Component {index} existiert nicht ({available} vorhanden)")]
    ComponentNotFound {
        /// Angefragter Index.
        index: usize,
        /// Anzahl vorhandener Components.
        available: usize,
    },
    /// ELI aus dem FRBR-Block war nicht valide.
    #[error("ungültiger ELI im FRBR-Block: {0}")]
    Id(#[from] IdError),
}
