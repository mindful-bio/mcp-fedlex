//! `TenantRepository` als einzige Schnittstelle zum Scratchpad (ADR-001).
//!
//! Aufrufer arbeiten nie direkt mit rohen Schlüsseln oder dem Store. Sie geben
//! einen [`TenantContext`] (aus dem validierten Claim) und einen rohen User-Key
//! an. Das Repository injiziert den Namespace und lehnt jede Key-Injection ab.
//! Damit ist die Tenant-Isolation eine Invariante des Data-Access-Layers, nicht
//! eine Konvention der Aufrufstellen.

use crate::key::{KeyError, TenantContext};
use crate::scratchpad::ScratchpadStore;

/// Einzige erlaubte Lese-/Schreib-Schnittstelle zum Agent-Scratchpad.
pub struct TenantRepository<S: ScratchpadStore> {
    store: S,
}

impl<S: ScratchpadStore> TenantRepository<S> {
    /// Erzeugt das Repository über einem konkreten Speicher.
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// Schreibt einen Wert in den genamespacten Bereich des Kontexts.
    pub fn put(&self, ctx: &TenantContext, user_key: &str, value: &str) -> Result<(), KeyError> {
        let key = ctx.key(user_key)?;
        self.store.put(&key, value);
        Ok(())
    }

    /// Liest einen Wert aus dem genamespacten Bereich des Kontexts.
    pub fn get(&self, ctx: &TenantContext, user_key: &str) -> Result<Option<String>, KeyError> {
        let key = ctx.key(user_key)?;
        Ok(self.store.get(&key))
    }

    /// Entfernt einen Wert aus dem genamespacten Bereich des Kontexts.
    pub fn delete(&self, ctx: &TenantContext, user_key: &str) -> Result<bool, KeyError> {
        let key = ctx.key(user_key)?;
        Ok(self.store.delete(&key))
    }

    /// Gibt eine Referenz auf den darunterliegenden Speicher (für Inspektion).
    pub fn store(&self) -> &S {
        &self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::{SessionId, TenantContext, TenantId};
    use crate::scratchpad::InMemoryScratchpad;

    fn ctx(t: &str, s: &str) -> TenantContext {
        TenantContext::new(
            TenantId::from_claim(t).unwrap(),
            SessionId::from_claim(s).unwrap(),
        )
    }

    #[test]
    fn roundtrip_within_one_tenant() {
        let repo = TenantRepository::new(InMemoryScratchpad::new());
        let a = ctx("kanzlei-a", "sess-1");
        repo.put(&a, "merkliste", "Art. 41 OR").unwrap();
        assert_eq!(
            repo.get(&a, "merkliste").unwrap().as_deref(),
            Some("Art. 41 OR")
        );
    }

    // ADR-001-Kernnachweis: gleicher User-Key, verschiedene Mandanten -> keine
    // Sichtbarkeit über die Grenze hinweg.
    #[test]
    fn cross_tenant_access_is_denied() {
        let repo = TenantRepository::new(InMemoryScratchpad::new());
        let a = ctx("kanzlei-a", "sess-1");
        let b = ctx("kanzlei-b", "sess-1");

        repo.put(&a, "fallnotiz", "vertraulich A").unwrap();

        // B nutzt denselben User-Key, sieht aber nichts von A.
        assert_eq!(repo.get(&b, "fallnotiz").unwrap(), None);
        // A sieht weiterhin sein eigenes Datum.
        assert_eq!(
            repo.get(&a, "fallnotiz").unwrap().as_deref(),
            Some("vertraulich A")
        );
    }

    // Auch verschiedene Sessions desselben Mandanten sind getrennt.
    #[test]
    fn cross_session_access_is_denied() {
        let repo = TenantRepository::new(InMemoryScratchpad::new());
        let s1 = ctx("kanzlei-a", "sess-1");
        let s2 = ctx("kanzlei-a", "sess-2");

        repo.put(&s1, "scratch", "nur sess-1").unwrap();
        assert_eq!(repo.get(&s2, "scratch").unwrap(), None);
    }

    // Ein Ausbruchs-Key (eingeschmuggelter Separator) schlägt hart fehl, statt
    // fremde Daten zu treffen.
    #[test]
    fn injection_key_fails_hard_instead_of_reaching_foreign_data() {
        let repo = TenantRepository::new(InMemoryScratchpad::new());
        let a = ctx("kanzlei-a", "sess-1");
        let b = ctx("kanzlei-b", "sess-1");
        repo.put(&b, "geheim", "Daten von B").unwrap();

        // A versucht, über Key-Injection in B's Namespace zu lesen.
        let attack = repo.get(&a, "kanzlei-b:sess-1:geheim");
        assert!(attack.is_err());
    }
}
