//! Schlüssel-Konstruktion und Tenant-Kontext (ADR-001).
//!
//! Die Tenant-Isolation des Scratchpads ist ein Enforcement-Problem, kein
//! Kryptografie-Problem. Sie steckt vollständig hier in der Schlüssel-Logik und
//! ist damit unabhängig vom konkreten Speicher (Redis, In-Memory) beweisbar.
//!
//! Zwei Regeln aus ADR-001:
//! 1. `tenant_id`/`session_id` stammen aus dem server-validierten Claim, niemals
//!    aus einem Tool-/LLM-Parameter. Der Typ [`TenantContext`] kann nur aus
//!    solchen Claim-Werten gebaut werden.
//! 2. Der vom LLM gelieferte Schlüssel-Anteil wird hart validiert. Separatoren
//!    (`:`) und Glob-Wildcards (`*`, `?`, `[`, `]`) werden abgelehnt, damit kein
//!    Ausbruch aus dem Namespace möglich ist (Redis-Key-Injection).

use std::fmt;

/// Trennzeichen des Namespace `tenant:session:key`.
const SEPARATOR: char = ':';

/// Zeichen, die in keinem Schlüssel-Bestandteil vorkommen dürfen.
/// `:` ist der Namespace-Separator, der Rest sind Redis-Glob-Wildcards.
const FORBIDDEN: &[char] = &[SEPARATOR, '*', '?', '[', ']'];

/// Fehler bei der Konstruktion eines Schlüssel-Bestandteils.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    /// Ein Bestandteil war leer oder nur Whitespace.
    #[error("key component must not be empty")]
    Empty,
    /// Ein Bestandteil enthielt ein verbotenes Zeichen (Separator oder Wildcard).
    #[error("key component contains forbidden character `{0}` (separator or glob wildcard)")]
    ForbiddenChar(char),
}

/// Validiert einen Schlüssel-Bestandteil gegen Leerstring und verbotene Zeichen.
fn validate_component(raw: &str) -> Result<(), KeyError> {
    if raw.trim().is_empty() {
        return Err(KeyError::Empty);
    }
    if let Some(bad) = raw.chars().find(|c| FORBIDDEN.contains(c)) {
        return Err(KeyError::ForbiddenChar(bad));
    }
    Ok(())
}

/// Mandanten-Kennung aus dem validierten Claim (nicht aus LLM-Parametern).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TenantId(String);

impl TenantId {
    /// Erzeugt eine Mandanten-Kennung aus einem Claim-Wert und validiert ihn.
    pub fn from_claim(raw: impl Into<String>) -> Result<Self, KeyError> {
        let raw = raw.into();
        validate_component(&raw)?;
        Ok(Self(raw))
    }

    /// Liefert die rohe Kennung.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Session-Kennung aus dem validierten Claim.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(String);

impl SessionId {
    /// Erzeugt eine Session-Kennung aus einem Claim-Wert und validiert ihn.
    pub fn from_claim(raw: impl Into<String>) -> Result<Self, KeyError> {
        let raw = raw.into();
        validate_component(&raw)?;
        Ok(Self(raw))
    }

    /// Liefert die rohe Kennung.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Der server-seitige Kontext, aus dem jeder Scratchpad-Schlüssel abgeleitet wird.
///
/// Nur aus Claim-validierten [`TenantId`]/[`SessionId`] konstruierbar. Damit kann
/// keine LLM-Halluzination den Namespace eines fremden Mandanten adressieren.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantContext {
    tenant: TenantId,
    session: SessionId,
}

impl TenantContext {
    /// Bündelt Mandant und Session zu einem Zugriffskontext.
    pub fn new(tenant: TenantId, session: SessionId) -> Self {
        Self { tenant, session }
    }

    /// Liefert die Mandanten-Kennung.
    pub fn tenant(&self) -> &TenantId {
        &self.tenant
    }

    /// Liefert die Session-Kennung.
    pub fn session(&self) -> &SessionId {
        &self.session
    }

    /// Leitet aus diesem Kontext und einem LLM-gelieferten Schlüssel den
    /// namespaced [`ScratchpadKey`] ab. Der User-Anteil wird hart validiert.
    pub fn key(&self, user_key: &str) -> Result<ScratchpadKey, KeyError> {
        validate_component(user_key)?;
        Ok(ScratchpadKey(format!(
            "{}{SEPARATOR}{}{SEPARATOR}{}",
            self.tenant.as_str(),
            self.session.as_str(),
            user_key
        )))
    }
}

/// Ein fertig genamespacter, injektionssicherer Scratchpad-Schlüssel.
///
/// Nur über [`TenantContext::key`] konstruierbar, daher trägt jeder Schlüssel
/// garantiert das Präfix `tenant:session:`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScratchpadKey(String);

impl ScratchpadKey {
    /// Liefert die rohe Schlüssel-Zeichenkette.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ScratchpadKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(t: &str, s: &str) -> TenantContext {
        TenantContext::new(
            TenantId::from_claim(t).unwrap(),
            SessionId::from_claim(s).unwrap(),
        )
    }

    #[test]
    fn builds_namespaced_key() {
        let key = ctx("kanzlei-a", "sess-1").key("notiz").unwrap();
        assert_eq!(key.as_str(), "kanzlei-a:sess-1:notiz");
    }

    #[test]
    fn rejects_empty_user_key() {
        assert_eq!(ctx("a", "s").key("  "), Err(KeyError::Empty));
    }

    #[test]
    fn rejects_separator_injection() {
        // Ausbruchsversuch: fremder Namespace via eingeschmuggeltem `:`.
        let attack = ctx("a", "s").key("kanzlei-b:sess-9:geheim");
        assert_eq!(attack, Err(KeyError::ForbiddenChar(':')));
    }

    #[test]
    fn rejects_glob_wildcards() {
        for bad in ['*', '?', '[', ']'] {
            let attempt = ctx("a", "s").key(&format!("notiz{bad}"));
            assert_eq!(attempt, Err(KeyError::ForbiddenChar(bad)));
        }
    }

    #[test]
    fn rejects_forged_tenant_id_with_separator() {
        // Auch der Claim-Pfad ist gehärtet (Defense-in-Depth).
        assert_eq!(
            TenantId::from_claim("a:b"),
            Err(KeyError::ForbiddenChar(':'))
        );
    }

    proptest::proptest! {
        // Jeder String mit einem verbotenen Zeichen wird abgelehnt.
        #[test]
        fn forbidden_chars_always_rejected(
            prefix in "[a-z0-9]{0,8}",
            bad in proptest::sample::select(FORBIDDEN.to_vec()),
            suffix in "[a-z0-9]{0,8}",
        ) {
            let user_key = format!("{prefix}{bad}{suffix}");
            let result = ctx("a", "s").key(&user_key);
            proptest::prop_assert!(result.is_err());
        }

        // Jeder nicht-leere String ohne verbotene Zeichen wird akzeptiert und
        // behält das korrekte Namespace-Präfix.
        #[test]
        fn clean_keys_always_accepted(user_key in "[a-zA-Z0-9_.-]{1,32}") {
            let key = ctx("tenant", "session").key(&user_key).unwrap();
            proptest::prop_assert!(key.as_str().starts_with("tenant:session:"));
            proptest::prop_assert!(key.as_str().ends_with(&user_key));
        }
    }
}
