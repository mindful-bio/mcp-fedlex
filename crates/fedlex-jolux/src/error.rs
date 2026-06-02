//! Fehlertyp der JOLux-Primitive.

use thiserror::Error;

/// Fehler bei einer JOLux-Primitive.
///
/// Die Primitive selbst geben `Result<_, JoluxError>` zurück. Die agentenseitige
/// Tool-Schicht übersetzt diese Fehler dann in lenkende `{ error, hint }`-Antworten
/// (Graceful Failure) — die Primitive bleiben ehrliche `Result`-Funktionen.
#[derive(Debug, Error)]
pub enum JoluxError {
    /// Transport-/Verbindungsfehler des SPARQL-Clients.
    #[error("SPARQL transport error: {0}")]
    Transport(String),

    /// Antwort war kein wohlgeformtes SPARQL-1.1-JSON.
    #[error("malformed SPARQL results: {0}")]
    MalformedResults(String),

    /// Erwartetes Ergebnis fehlte (leeres Binding-Set).
    #[error("no result for `{0}`")]
    NotFound(String),

    /// Ungültiger Identifier (ELI/ECLI).
    #[error(transparent)]
    Id(#[from] fedlex_core::IdError),
}
