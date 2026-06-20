//! Protokoll-Versions-Negotiation (Migrations-Runbook Phase 2/6, ADR-008).
//!
//! Der Reader bedient zwei Revisionen: die historische `2024-11-05` (für
//! Negotiation mit explizit nachfragenden Alt-Clients) und — als **Default** —
//! die Ziel-Revision **`2025-11-25`**. Diese Schicht macht die Aushandlung
//! **explizit und erweiterbar**. Die Menge der echt unterstützten Versionen
//! ([`SUPPORTED_PROTOCOL_VERSIONS`]) wächst erst, wenn die jeweilige Revision
//! tatsächlich implementiert und getestet ist (Lifecycle: `initialize`,
//! `notifications/initialized`, `ping`) — Capabilities bleiben ehrlich.

//!
//! Negotiation-Regeln (Runbook 2.2):
//! - Client nennt eine **unterstützte** Version → diese wird ausgehandelt.
//! - Client nennt **keine** Version (heutiger ansV-Fall) → die **Default**-Version.
//! - Client nennt eine **unbekannte/zu neue** Version → die **höchste eigene**
//!   (spec-konform: kein harter Fehler, der Server bietet sein Bestes an).
//!
//! So bleibt der Default ein **Config-Flip** (Runbook 2.3, 6.2), nicht ein
//! Code-Redeploy: Der Sprung der ausgehandelten Default-Version ist später eine
//! reine Konfigurationsänderung.

/// Alle Protokollrevisionen, die der Reader **tatsächlich bedient** — aufsteigend
/// sortiert (älteste zuerst, neueste zuletzt). Single Source of Truth für den
/// `initialize`-Handshake und die Konsistenztests.
///
/// Wächst pro Migrationsphase, **erst nachdem** die Revision implementiert und
/// getestet ist (kein vorauseilendes „Support"-Versprechen). Beide Einträge
/// sind durch den vollständigen Lifecycle (`initialize`/`notifications/
/// initialized`/`ping`) gedeckt; `2024-11-05` bleibt nur für die Aushandlung
/// mit explizit nachfragenden Alt-Clients.
pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2024-11-05", "2025-11-25"];

/// Die ausgehandelte Default-Version, wenn der Client keine nennt. Auf die
/// Ziel-Revision `2025-11-25` gehoben (Runbook Phase 6). Die handshake-losen
/// Clients (ansV, syllogismus-fedlex) lesen `protocolVersion` nicht aus und
/// bleiben unberührt; explizit nachfragende Clients erhalten weiterhin die von
/// ihnen genannte unterstützte Version. Per [`default_protocol_version`] aus der
/// Umgebung überschreibbar (Runbook 2.3), z. B. Rollback auf `2024-11-05`.
pub const DEFAULT_PROTOCOL_VERSION: &str = "2025-11-25";


/// Die höchste tatsächlich unterstützte Revision (letztes Element der sortierten
/// Liste). Antwort auf unbekannte/zu neue Client-Versionen.
pub fn highest_supported() -> &'static str {
    SUPPORTED_PROTOCOL_VERSIONS
        .last()
        .copied()
        // Die Konstante ist nie leer; defensiv dennoch ein fester Fallback.
        .unwrap_or(DEFAULT_PROTOCOL_VERSION)
}

/// Prüft, ob `version` echt unterstützt wird.
pub fn is_supported(version: &str) -> bool {
    SUPPORTED_PROTOCOL_VERSIONS.contains(&version)
}

/// Liefert die effektive Default-Version: `MCP_PROTOCOL_DEFAULT` aus der Umgebung,
/// falls gesetzt **und** unterstützt; sonst die Kompilier-Default
/// ([`DEFAULT_PROTOCOL_VERSION`]).
///
/// Eine gesetzte, aber **nicht** unterstützte Override wird **ignoriert**
/// (fail-safe statt Aushandeln einer Version, die wir nicht bedienen).
pub fn default_protocol_version() -> &'static str {
    match std::env::var("MCP_PROTOCOL_DEFAULT") {
        Ok(v) => match SUPPORTED_PROTOCOL_VERSIONS.iter().find(|s| **s == v) {
            Some(s) => s,
            None => DEFAULT_PROTOCOL_VERSION,
        },
        Err(_) => DEFAULT_PROTOCOL_VERSION,
    }
}

/// Handelt die Protokollversion gegen die Client-Angabe aus, mit explizit
/// gegebener Default-Version (testbar, ohne Umgebung).
///
/// `requested` ist der Wert von `params.protocolVersion` aus `initialize`
/// (`None`, wenn der Client keinen sendet — heutiger ansV-Fall).
pub fn negotiate(requested: Option<&str>, default: &'static str) -> &'static str {
    match requested {
        // Unterstützt → genau diese aushandeln (als 'static aus der Liste).
        Some(v) if is_supported(v) => SUPPORTED_PROTOCOL_VERSIONS
            .iter()
            .find(|s| **s == v)
            .copied()
            .unwrap_or(default),
        // Genannt, aber unbekannt/zu neu → unser Bestes anbieten.
        Some(_) => highest_supported(),
        // Nichts genannt → konfigurierte Default-Version.
        None => default,
    }
}

/// Klassifikation des HTTP-Headers `MCP-Protocol-Version` (Migrations-Runbook
/// Phase 3, Reconciliation 3.1↔6).
///
/// **Wichtige Abgrenzung zur `initialize`-Negotiation:** Dies ist die
/// **Header-Ebene** des Streamable-HTTP-Transports (Spec `2025-06-18` #8,
/// `2025-11-25` #12), **nicht** der `initialize`-Handshake (das ist
/// [`negotiate`]). Beide Ebenen haben bewusst **unterschiedliche** Fallbacks,
/// und genau dieser Unterschied ist die in 3.1 markierte Stolperfalle:
/// - `initialize` ohne `protocolVersion` → [`negotiate`] gibt die **Default**
///   (heute `2024-11-05`).
/// - HTTP-Header **fehlt** → Spec-SHOULD: Rückwärtskompatibilität, **kein
///   Fehler**. Für die beiden Alt-Clients (ansV, syllogismus-fedlex), die den
///   Header nie senden, ist [`ProtocolHeaderOutcome::Absent`] der Normalfall.
///
/// **Diese Funktion ist reine Klassifikation und (noch) NICHT in den
/// Request-Pfad verdrahtet.** Der `Unsupported`-Fall entspricht dem späteren
/// HTTP **400**, wird aber erst dann live geschaltet, wenn der Streamable-HTTP-
/// Pfad steht und die Ziel-Revision tatsächlich in
/// [`SUPPORTED_PROTOCOL_VERSIONS`] aufgenommen ist. Bis dahin bleibt das
/// Verhalten für Alt-Clients unverändert (kein Header-Zwang, kein 400).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolHeaderOutcome {
    /// Header fehlt. Spec-SHOULD: Server nimmt Rückwärtskompatibilität an,
    /// **kein** Fehler. Normalfall für die heutigen, header-losen Alt-Clients.
    Absent,
    /// Header nennt eine **echt unterstützte** Version → diese gilt
    /// (als `'static` aus der Support-Liste).
    Supported(&'static str),
    /// Header ist gesetzt, nennt aber eine **nicht unterstützte** Version.
    /// Spec verlangt hier HTTP **400** — bewusst noch nicht live verdrahtet
    /// (siehe Typ-Doku), bis Streamable HTTP aktiv ist.
    Unsupported,
}

/// Klassifiziert den Roh-Wert des Headers `MCP-Protocol-Version`.
///
/// `header` ist `None`, wenn der Header fehlt (heutiger Alt-Client-Fall), sonst
/// sein getrimmter Wert. Ein **leerer** Header-Wert wird wie „fehlt" behandelt
/// (defensiv: kein 400 für einen leeren String, der faktisch keine Angabe ist).
pub fn classify_protocol_header(header: Option<&str>) -> ProtocolHeaderOutcome {
    match header.map(str::trim) {
        // Fehlt oder leer → kein Zwang, Rückwärtskompatibilität (Spec-SHOULD).
        None | Some("") => ProtocolHeaderOutcome::Absent,
        // Gesetzt + unterstützt → exakt diese Version (als 'static aus der Liste).
        Some(v) => match SUPPORTED_PROTOCOL_VERSIONS.iter().find(|s| **s == v) {
            Some(s) => ProtocolHeaderOutcome::Supported(s),
            // Gesetzt, aber unbekannt → der spätere 400-Fall.
            None => ProtocolHeaderOutcome::Unsupported,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_list_is_sorted_ascending_and_nonempty() {
        assert!(!SUPPORTED_PROTOCOL_VERSIONS.is_empty());
        let mut sorted = SUPPORTED_PROTOCOL_VERSIONS.to_vec();
        sorted.sort_unstable();
        assert_eq!(
            sorted, SUPPORTED_PROTOCOL_VERSIONS,
            "Versionsliste muss aufsteigend sortiert sein (älteste … neueste)"
        );
    }

    #[test]
    fn default_is_itself_supported() {
        assert!(
            is_supported(DEFAULT_PROTOCOL_VERSION),
            "Default-Version muss in der Support-Liste stehen"
        );
    }

    #[test]
    fn highest_is_the_last_entry() {
        assert_eq!(
            highest_supported(),
            *SUPPORTED_PROTOCOL_VERSIONS.last().unwrap()
        );
    }

    #[test]
    fn missing_client_version_yields_default() {
        // Heutiger ansV-Fall: kein protocolVersion im initialize.
        assert_eq!(
            negotiate(None, DEFAULT_PROTOCOL_VERSION),
            DEFAULT_PROTOCOL_VERSION
        );
    }

    #[test]
    fn known_client_version_is_echoed() {
        assert_eq!(
            negotiate(Some("2024-11-05"), DEFAULT_PROTOCOL_VERSION),
            "2024-11-05"
        );
    }

    #[test]
    fn unknown_or_too_new_version_falls_back_to_highest() {
        // Spec-konform: kein harter Fehler, sondern „unser Bestes".
        assert_eq!(
            negotiate(Some("2099-01-01"), DEFAULT_PROTOCOL_VERSION),
            highest_supported()
        );
        assert_eq!(
            negotiate(Some("garbage"), DEFAULT_PROTOCOL_VERSION),
            highest_supported()
        );
    }

    // --- HTTP-Header `MCP-Protocol-Version` (Phase 3, noch nicht verdrahtet) ---

    #[test]
    fn header_absent_is_backward_compatible_not_an_error() {
        // Kern der Reconciliation 3.1↔6: fehlender Header darf die heutigen
        // Alt-Clients (ansV, syllogismus-fedlex) NICHT brechen.
        assert_eq!(
            classify_protocol_header(None),
            ProtocolHeaderOutcome::Absent
        );
    }

    #[test]
    fn header_empty_or_whitespace_is_treated_as_absent() {
        // Ein leerer/whitespace-Header ist faktisch keine Angabe → kein 400.
        assert_eq!(
            classify_protocol_header(Some("")),
            ProtocolHeaderOutcome::Absent
        );
        assert_eq!(
            classify_protocol_header(Some("   ")),
            ProtocolHeaderOutcome::Absent
        );
    }

    #[test]
    fn header_supported_version_is_accepted_as_static() {
        match classify_protocol_header(Some("2024-11-05")) {
            ProtocolHeaderOutcome::Supported(v) => assert_eq!(v, "2024-11-05"),
            other => panic!("erwartet Supported, war {other:?}"),
        }
    }

    #[test]
    fn header_surrounding_whitespace_is_trimmed_before_match() {
        assert_eq!(
            classify_protocol_header(Some("  2024-11-05  ")),
            ProtocolHeaderOutcome::Supported("2024-11-05")
        );
    }

    #[test]
    fn header_unknown_version_is_classified_unsupported_future_400() {
        // Gesetzt, aber nicht unterstützt → der spätere HTTP-400-Fall.
        // (Klassifikation; live-Verdrahtung erst mit Streamable HTTP.)
        assert_eq!(
            classify_protocol_header(Some("2099-01-01")),
            ProtocolHeaderOutcome::Unsupported
        );
        assert_eq!(
            classify_protocol_header(Some("garbage")),
            ProtocolHeaderOutcome::Unsupported
        );
    }
}
