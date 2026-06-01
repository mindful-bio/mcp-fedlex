//! Allowlist-basierter PII-Scrubber (ADR-001, Entscheidung 1).
//!
//! Sobald ein Agent einen konkreten Fall analysiert, fliessen Mandanten-PII
//! (Namen, Firmen, Aktenzeichen) durch Tool-Argumente und Responses. Diese
//! dürfen niemals unmaskiert in ein (ggf. Cloud-)Observability-Backend gelangen.
//!
//! Die Vertrauensgrenze ist eine Allowlist, keine Blocklist. Nur explizit
//! freigegebene Span-Attribute werden exportiert, alles andere ist per Default
//! redacted. Regex-Blocklisten für Namen lecken zuverlässig und sind hier
//! unzulässig. Die Redaction passiert am Entstehungspunkt (beim Bau des Spans),
//! nicht erst im Exporter.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// Platzhalter, der anstelle nicht freigegebener Werte exportiert wird.
pub const REDACTED: &str = "[REDACTED]";

/// Allowlist freigegebener Span-Attribut-Schlüssel.
///
/// Nur hier eingetragene Schlüssel verlassen den Prozess im Klartext. Alle
/// übrigen Attribute werden redacted, unabhängig von ihrem Inhalt. Roh-Tool-
/// Argumente und Roh-Responses stehen bewusst nie auf der Allowlist.
#[derive(Debug, Clone, Default)]
pub struct AttributeAllowlist {
    allowed: BTreeSet<String>,
}

impl AttributeAllowlist {
    /// Erzeugt eine leere Allowlist (alles wird redacted).
    pub fn new() -> Self {
        Self::default()
    }

    /// Konservative Default-Allowlist für unkritische, technische Attribute.
    ///
    /// Bewusst eng. Enthält nur Felder ohne PII-Bezug (Pool, Tool-Name, Rolle,
    /// ELI, Stichtag, Dauer, Status). Keine Tool-Argumente, keine Response-Inhalte.
    pub fn baseline() -> Self {
        let mut allowed = BTreeSet::new();
        for key in [
            "tool.name",
            "tool.pool",
            "auth.role",
            "provenance.eli",
            "provenance.valid_as_of",
            "http.status",
            "span.duration_ms",
            "outcome",
        ] {
            allowed.insert(key.to_string());
        }
        Self { allowed }
    }

    /// Fügt einen freigegebenen Schlüssel hinzu (Builder-Stil).
    pub fn allow(mut self, key: impl Into<String>) -> Self {
        self.allowed.insert(key.into());
        self
    }

    /// Ob ein Schlüssel exportiert werden darf.
    pub fn permits(&self, key: &str) -> bool {
        self.allowed.contains(key)
    }
}

/// Ein export-bereiter Span, dessen Attribute die Vertrauensgrenze passiert haben.
///
/// Nur über [`SpanScrubber::scrub`] baubar. Die einzige Art, an die exportierten
/// Attribute zu kommen, führt damit zwingend durch die Allowlist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrubbedSpan {
    name: String,
    attributes: BTreeMap<String, String>,
}

impl ScrubbedSpan {
    /// Name des Spans.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Die freigegebenen, export-bereiten Attribute.
    pub fn attributes(&self) -> &BTreeMap<String, String> {
        &self.attributes
    }

    /// Wert eines exportierten Attributs.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.attributes.get(key).map(String::as_str)
    }
}

/// Scrubber, der rohe Span-Attribute gegen die Allowlist filtert.
#[derive(Debug, Clone)]
pub struct SpanScrubber {
    allowlist: AttributeAllowlist,
}

impl SpanScrubber {
    /// Erzeugt einen Scrubber über einer Allowlist.
    pub fn new(allowlist: AttributeAllowlist) -> Self {
        Self { allowlist }
    }

    /// Maskiert rohe Attribute am Entstehungspunkt.
    ///
    /// Jeder Schlüssel, der nicht auf der Allowlist steht, wird mit [`REDACTED`]
    /// ersetzt. Der Schlüssel selbst bleibt erhalten (für Debugging der
    /// Span-Form), nur der Wert ist maskiert. So ist sichtbar, dass ein Attribut
    /// existierte, ohne seinen Inhalt zu lecken.
    pub fn scrub(
        &self,
        name: impl Into<String>,
        raw_attributes: impl IntoIterator<Item = (String, String)>,
    ) -> ScrubbedSpan {
        let attributes = raw_attributes
            .into_iter()
            .map(|(key, value)| {
                if self.allowlist.permits(&key) {
                    (key, value)
                } else {
                    (key, REDACTED.to_string())
                }
            })
            .collect();
        ScrubbedSpan {
            name: name.into(),
            attributes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn scrubber() -> SpanScrubber {
        SpanScrubber::new(AttributeAllowlist::baseline())
    }

    #[test]
    fn allowed_attributes_pass_through() {
        let span = scrubber().scrub(
            "tool.execute",
            [
                ("tool.name".to_string(), "search".to_string()),
                ("auth.role".to_string(), "Reader".to_string()),
            ],
        );
        assert_eq!(span.get("tool.name"), Some("search"));
        assert_eq!(span.get("auth.role"), Some("Reader"));
    }

    #[test]
    fn raw_tool_argument_is_redacted() {
        // Ein Tool-Argument mit Mandantenname. Darf nie im Klartext exportiert werden.
        let span = scrubber().scrub(
            "tool.execute",
            [(
                "tool.arguments".to_string(),
                "Mandant Müller AG, Az. 2024-117".to_string(),
            )],
        );
        assert_eq!(span.get("tool.arguments"), Some(REDACTED));
    }

    #[test]
    fn raw_response_is_redacted() {
        let span = scrubber().scrub(
            "tool.execute",
            [(
                "tool.response".to_string(),
                "Klartext-Rechtsgutachten zu Fall Meier".to_string(),
            )],
        );
        assert_eq!(span.get("tool.response"), Some(REDACTED));
    }

    #[test]
    fn empty_allowlist_redacts_everything() {
        let scrubber = SpanScrubber::new(AttributeAllowlist::new());
        let span = scrubber.scrub("any", [("tool.name".to_string(), "search".to_string())]);
        assert_eq!(span.get("tool.name"), Some(REDACTED));
    }

    proptest! {
        // Property. Egal welcher Schlüssel und Wert, ein nicht freigegebener
        // Schlüssel führt nie zum Klartext-Export. Das ist der ADR-001-Nachweis.
        #[test]
        fn no_non_allowlisted_value_ever_leaks(
            key in "[a-z][a-z0-9_.]{0,30}",
            value in ".{0,200}",
        ) {
            let scrubber = scrubber();
            let span = scrubber.scrub("s", [(key.clone(), value.clone())]);
            let allowlist = AttributeAllowlist::baseline();
            if allowlist.permits(&key) {
                prop_assert_eq!(span.get(&key), Some(value.as_str()));
            } else {
                // Nicht freigegeben. Der exportierte Wert ist immer der Platzhalter,
                // niemals der rohe Wert (ausser der rohe Wert wäre zufällig genau
                // der Platzhalter, was die Invariante nicht verletzt).
                prop_assert_eq!(span.get(&key), Some(REDACTED));
            }
        }

        // Property. Roh-Tool-Argumente und Roh-Responses sind nie freigegeben,
        // ihr Inhalt erscheint niemals im exportierten Span.
        #[test]
        fn tool_payload_keys_are_always_redacted(
            payload in ".{0,500}",
        ) {
            let scrubber = scrubber();
            for key in ["tool.arguments", "tool.response", "scratchpad.value", "session.payload"] {
                let span = scrubber.scrub("s", [(key.to_string(), payload.clone())]);
                prop_assert_eq!(span.get(key), Some(REDACTED));
            }
        }
    }
}
