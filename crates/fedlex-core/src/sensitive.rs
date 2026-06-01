//! `Sensitive<T>` als Typsystem-Schutz gegen PII-Lecks (ADR-001).
//!
//! Mandanten-PII (Namen, Firmen, Aktenzeichen) darf niemals versehentlich in
//! einen Trace, ein Log oder eine Fehlermeldung gelangen. Dieser Newtype
//! redacted seine `Debug`- und `Display`-Ausgabe, sodass ein achtloses
//! `tracing::info!(?feld)` oder `format!("{}", feld)` nur den Platzhalter zeigt.
//! Der Klartext ist ausschliesslich über das explizite [`Sensitive::expose`]
//! erreichbar, was Aufrufstellen sichtbar und auditierbar macht.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Platzhalter, der anstelle des Klartexts ausgegeben wird.
const REDACTED: &str = "[REDACTED]";

/// Hülle für vertrauliche Werte. `Debug`/`Display` zeigen nie den Klartext.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Sensitive<T>(T);

impl<T> Sensitive<T> {
    /// Markiert einen Wert als vertraulich.
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// Gibt den Klartext bewusst frei. Aufrufstellen sind damit auditierbar.
    pub fn expose(&self) -> &T {
        &self.0
    }

    /// Entnimmt den Klartext und konsumiert die Hülle.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> fmt::Debug for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

impl<T> fmt::Display for Sensitive<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

impl<T> From<T> for Sensitive<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_is_redacted() {
        let s = Sensitive::new("Muster AG");
        assert_eq!(format!("{s:?}"), "[REDACTED]");
        assert!(!format!("{s:?}").contains("Muster"));
    }

    #[test]
    fn display_is_redacted() {
        let s = Sensitive::new("Aktenzeichen 4A_1/2020");
        assert_eq!(format!("{s}"), "[REDACTED]");
        assert!(!format!("{s}").contains("4A_1"));
    }

    #[test]
    fn expose_returns_plaintext() {
        let s = Sensitive::new("Hans Muster".to_string());
        assert_eq!(s.expose(), "Hans Muster");
    }

    #[test]
    fn into_inner_consumes_and_returns() {
        let s = Sensitive::new(42u32);
        assert_eq!(s.into_inner(), 42);
    }

    // ADR-001-Nachweis: selbst eingebettet in eine Struktur leckt der Debug-
    // Output kein PII, weil das Feld die redactende Debug-Impl erbt.
    #[test]
    fn nested_struct_does_not_leak() {
        #[derive(Debug)]
        struct Span {
            #[allow(dead_code)]
            client: Sensitive<String>,
            #[allow(dead_code)]
            tool: &'static str,
        }
        let span = Span {
            client: Sensitive::new("Geheim AG".to_string()),
            tool: "query_law",
        };
        let rendered = format!("{span:?}");
        assert!(!rendered.contains("Geheim"));
        assert!(rendered.contains("query_law"));
    }
}
