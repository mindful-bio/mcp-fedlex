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

/// Boxed-Variante, damit main.rs den Resolver zur Laufzeit wählen kann
/// (JWT-Modus oder Dev-Token), ohne McpService umzubauen.
impl AuthResolver for Box<dyn AuthResolver + Send + Sync> {
    fn verify(&self, credential: &str) -> Result<VerifiedClaims, AuthError> {
        (**self).verify(credential)
    }
}

/// Die erwartete Claim-Form eines IdP-Tokens.
///
/// `exp` und `iss` prüft die `jsonwebtoken`-Validation, hier landen nur die
/// fachlichen Felder. Mandant und Session kommen aus dem signierten Token,
/// nie aus Tool-Parametern (ADR-002).
#[derive(Debug, serde::Deserialize)]
struct JwtClaims {
    /// Mandanten-Kennung.
    tenant: String,
    /// Session-Kennung.
    sid: String,
    /// Rolle als String (`reader`, `navigator`, `validator`).
    role: String,
}

/// JWT-basierter Resolver für den IdP-Anschluss.
///
/// Verifiziert signierte Tokens mit statischem Schlüsselmaterial (HS256-Secret
/// oder RS256-Public-Key im PEM-Format). Issuer ist Pflicht, Audience
/// optional. JWKS-Rotation ist bewusst noch nicht dabei, der Schlüssel kommt
/// als Secret ins Deployment und der Trait bleibt synchron.
///
/// Alle Token-Fehler kollabieren nach aussen zu [`AuthError::InvalidCredential`],
/// damit ein Angreifer aus der Fehlermeldung nichts über Signatur, Ablauf oder
/// Claim-Form lernt.
pub struct JwtAuthResolver {
    key: jsonwebtoken::DecodingKey,
    validation: jsonwebtoken::Validation,
}

impl JwtAuthResolver {
    /// HS256-Variante mit gemeinsamem Secret.
    pub fn hs256(secret: &[u8], issuer: &str, audience: Option<&str>) -> Self {
        Self {
            key: jsonwebtoken::DecodingKey::from_secret(secret),
            validation: Self::validation(jsonwebtoken::Algorithm::HS256, issuer, audience),
        }
    }

    /// RS256-Variante mit Public-Key im PEM-Format.
    pub fn rs256_pem(pem: &[u8], issuer: &str, audience: Option<&str>) -> Result<Self, AuthError> {
        let key = jsonwebtoken::DecodingKey::from_rsa_pem(pem)
            .map_err(|_| AuthError::InvalidCredential)?;
        Ok(Self {
            key,
            validation: Self::validation(jsonwebtoken::Algorithm::RS256, issuer, audience),
        })
    }

    fn validation(
        alg: jsonwebtoken::Algorithm,
        issuer: &str,
        audience: Option<&str>,
    ) -> jsonwebtoken::Validation {
        let mut v = jsonwebtoken::Validation::new(alg);
        v.set_issuer(&[issuer]);
        match audience {
            Some(aud) => v.set_audience(&[aud]),
            None => v.validate_aud = false,
        }
        v
    }

    fn role_from_claim(raw: &str) -> Result<Role, AuthError> {
        match raw {
            "reader" => Ok(Role::Reader),
            "navigator" => Ok(Role::Navigator),
            "validator" => Ok(Role::Validator),
            _ => Err(AuthError::InvalidCredential),
        }
    }
}

impl AuthResolver for JwtAuthResolver {
    fn verify(&self, credential: &str) -> Result<VerifiedClaims, AuthError> {
        let token = jsonwebtoken::decode::<JwtClaims>(credential, &self.key, &self.validation)
            .map_err(|_| AuthError::InvalidCredential)?;

        // Auch signierte Claims durchlaufen die Schlüssel-Invarianten
        // (Defense-in-Depth gegen fehlkonfigurierte IdPs).
        let tenant = TenantId::from_claim(token.claims.tenant)?;
        let session = SessionId::from_claim(token.claims.sid)?;
        let role = Self::role_from_claim(&token.claims.role)?;

        Ok(VerifiedClaims {
            tenant,
            session,
            role,
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

    // --- JwtAuthResolver -------------------------------------------------

    const JWT_SECRET: &[u8] = b"test-secret";
    const ISSUER: &str = "https://idp.example";

    fn jwt_resolver() -> JwtAuthResolver {
        JwtAuthResolver::hs256(JWT_SECRET, ISSUER, Some("mcp-fedlex"))
    }

    fn sign(claims: serde_json::Value) -> String {
        jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(JWT_SECRET),
        )
        .unwrap()
    }

    fn valid_claims() -> serde_json::Value {
        serde_json::json!({
            "iss": ISSUER,
            "aud": "mcp-fedlex",
            "exp": 4_102_444_800u64, // 2100, weit in der Zukunft
            "tenant": "kanzlei-a",
            "sid": "sess-1",
            "role": "navigator",
        })
    }

    #[test]
    fn jwt_valid_token_yields_verified_claims() {
        let claims = jwt_resolver().verify(&sign(valid_claims())).unwrap();
        assert_eq!(claims.tenant().as_str(), "kanzlei-a");
        assert_eq!(claims.session().as_str(), "sess-1");
        assert_eq!(claims.role(), Role::Navigator);
    }

    #[test]
    fn jwt_wrong_signature_is_rejected() {
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
            &valid_claims(),
            &jsonwebtoken::EncodingKey::from_secret(b"other-secret"),
        )
        .unwrap();
        assert!(matches!(
            jwt_resolver().verify(&token),
            Err(AuthError::InvalidCredential)
        ));
    }

    #[test]
    fn jwt_expired_token_is_rejected() {
        let mut c = valid_claims();
        c["exp"] = serde_json::json!(946_684_800u64); // 2000, abgelaufen
        assert!(matches!(
            jwt_resolver().verify(&sign(c)),
            Err(AuthError::InvalidCredential)
        ));
    }

    #[test]
    fn jwt_wrong_issuer_or_audience_is_rejected() {
        let mut c = valid_claims();
        c["iss"] = serde_json::json!("https://evil.example");
        assert!(jwt_resolver().verify(&sign(c)).is_err());

        let mut c = valid_claims();
        c["aud"] = serde_json::json!("other-service");
        assert!(jwt_resolver().verify(&sign(c)).is_err());
    }

    #[test]
    fn jwt_unknown_role_is_rejected() {
        let mut c = valid_claims();
        c["role"] = serde_json::json!("admin");
        assert!(matches!(
            jwt_resolver().verify(&sign(c)),
            Err(AuthError::InvalidCredential)
        ));
    }

    #[test]
    fn jwt_malformed_tenant_fails_key_invariants() {
        let mut c = valid_claims();
        c["tenant"] = serde_json::json!("kanzlei:a"); // Trennzeichen verboten
        assert!(matches!(
            jwt_resolver().verify(&sign(c)),
            Err(AuthError::MalformedClaim(_))
        ));
    }

    #[test]
    fn boxed_resolver_dispatches() {
        let boxed: Box<dyn AuthResolver + Send + Sync> = Box::new(jwt_resolver());
        assert!(boxed.verify(&sign(valid_claims())).is_ok());
        assert!(boxed.verify("kein-jwt").is_err());
    }
}
