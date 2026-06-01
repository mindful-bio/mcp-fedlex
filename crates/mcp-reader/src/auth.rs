//! Auth & RBAC-Resolver (ADR-002 / Least-Privilege).
//!
//! Der Agent authentifiziert sich beim Connect. Aus dem server-validierten
//! Claim entstehen Mandant, Session und Rolle. Entscheidend. Diese Werte
//! kommen NIE aus einem Tool- oder LLM-Parameter, sonst könnte eine
//! Halluzination Mandant oder Quota fälschen. Der reale IdP/Vault wird hier
//! über den [`AuthResolver`]-Trait abstrahiert (im Test gemockt).

use fedlex_store::{KeyError, SessionId, TenantId};
use std::collections::HashMap;

/// RBAC-Rolle, abgeleitet aus dem validierten Claim. Bestimmt Tool-Pool und
/// Quota-Klasse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// Nur lesende Navigation.
    Reader,
    /// Navigation plus föderierte Auflösung.
    Navigator,
    /// Zusätzlich Validierungs-Tools.
    Validator,
}

/// Der server-validierte Identitäts-Kontext eines Agenten.
///
/// Nur aus einem geprüften Claim konstruierbar. `tenant` und `session` sind
/// damit vertrauenswürdig und nicht durch LLM-Eingaben manipulierbar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedClaims {
    tenant: TenantId,
    session: SessionId,
    role: Role,
}

impl VerifiedClaims {
    /// Liefert den Mandanten.
    pub fn tenant(&self) -> &TenantId {
        &self.tenant
    }

    /// Liefert die Session.
    pub fn session(&self) -> &SessionId {
        &self.session
    }

    /// Liefert die Rolle.
    pub fn role(&self) -> Role {
        self.role
    }
}

/// Fehler der Authentifizierung.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
    /// Credential unbekannt oder ungültig.
    #[error("invalid credential")]
    InvalidCredential,
    /// Ein Claim-Bestandteil verletzte die Schlüssel-Invarianten.
    #[error("malformed claim: {0}")]
    MalformedClaim(#[from] KeyError),
}

/// Abstraktion über IdP/Vault. Validiert ein Credential und liefert den Claim.
pub trait AuthResolver {
    /// Validiert das Connect-Credential und liefert den geprüften Claim.
    fn verify(&self, credential: &str) -> Result<VerifiedClaims, AuthError>;
}

/// Ein Roh-Claim, wie ihn ein IdP nach erfolgreicher Validierung liefert.
#[derive(Debug, Clone)]
pub struct ClaimRecord {
    /// Mandanten-Kennung aus dem Claim.
    pub tenant: String,
    /// Session-Kennung aus dem Claim.
    pub session: String,
    /// Rolle aus dem Claim.
    pub role: Role,
}

/// In-Memory-Resolver. Steht stellvertretend für IdP/Vault und ist im Test
/// frei konfigurierbar. Mappt Credential -> Claim.
#[derive(Debug, Default, Clone)]
pub struct StaticAuthResolver {
    records: HashMap<String, ClaimRecord>,
}

impl StaticAuthResolver {
    /// Erzeugt einen leeren Resolver.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registriert ein Credential mit zugehörigem Claim.
    pub fn with_credential(mut self, credential: impl Into<String>, record: ClaimRecord) -> Self {
        self.records.insert(credential.into(), record);
        self
    }
}

impl AuthResolver for StaticAuthResolver {
    fn verify(&self, credential: &str) -> Result<VerifiedClaims, AuthError> {
        let record = self
            .records
            .get(credential)
            .ok_or(AuthError::InvalidCredential)?;

        // Auch der Claim-Pfad wird hart validiert (Defense-in-Depth).
        let tenant = TenantId::from_claim(record.tenant.clone())?;
        let session = SessionId::from_claim(record.session.clone())?;

        Ok(VerifiedClaims {
            tenant,
            session,
            role: record.role,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver() -> StaticAuthResolver {
        StaticAuthResolver::new().with_credential(
            "secret-token-a",
            ClaimRecord {
                tenant: "kanzlei-a".into(),
                session: "sess-1".into(),
                role: Role::Navigator,
            },
        )
    }

    #[test]
    fn verifies_known_credential() {
        let claims = resolver().verify("secret-token-a").unwrap();
        assert_eq!(claims.tenant().as_str(), "kanzlei-a");
        assert_eq!(claims.session().as_str(), "sess-1");
        assert_eq!(claims.role(), Role::Navigator);
    }

    #[test]
    fn rejects_unknown_credential() {
        assert_eq!(
            resolver().verify("forged"),
            Err(AuthError::InvalidCredential)
        );
    }

    #[test]
    fn rejects_malformed_claim() {
        // Ein IdP-Claim mit Separator im Mandanten wird hart abgewiesen.
        let r = StaticAuthResolver::new().with_credential(
            "bad",
            ClaimRecord {
                tenant: "kanzlei-a:evil".into(),
                session: "sess-1".into(),
                role: Role::Reader,
            },
        );
        assert!(matches!(r.verify("bad"), Err(AuthError::MalformedClaim(_))));
    }
}
