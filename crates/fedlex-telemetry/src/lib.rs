//! fedlex-telemetry - Tracing-Layer und PII-Scrubber (Compliance-Gate).
//!
//! Ab M9 trägt der Crate den allowlist-basierten PII-Scrubber ([`scrubber`],
//! ADR-001) als auditierbare Vertrauensgrenze. Nur explizit freigegebene
//! Span-Attribute verlassen den Prozess, alles andere ist per Default redacted.
//! Die typsystem-seitige Redaction am Entstehungspunkt liefert [`Sensitive`] aus
//! fedlex-core, dessen `Debug`/`Display` nie Klartext zeigen.

#![forbid(unsafe_code)]

pub mod scrubber;

pub use fedlex_core::Sensitive;
pub use scrubber::{AttributeAllowlist, ScrubbedSpan, SpanScrubber, REDACTED};

/// Name des Crates, dient als Smoke-Test-Anker.
pub const CRATE_NAME: &str = "fedlex-telemetry";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_crate_name() {
        assert_eq!(CRATE_NAME, "fedlex-telemetry");
    }
}
