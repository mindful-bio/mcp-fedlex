//! Scratchpad-Speicher als Trait plus In-Memory-Implementierung.
//!
//! Das Trait abstrahiert den L2-Speicher (in Produktion Redis). Die Isolation
//! liegt im Schlüssel (siehe [`crate::key`]), daher beweist die In-Memory-
//! Implementierung dieselbe Invariante wie ein echtes Redis und läuft ohne
//! Infrastruktur in der CI. Eine Redis-Implementierung dockt später hinter
//! demselben Trait an (Docker-gated Integrationstest).

use crate::key::ScratchpadKey;
use std::collections::HashMap;
use std::sync::Mutex;

/// Abstrakter Scratchpad-Speicher. Operiert ausschliesslich auf fertig
/// genamespacten [`ScratchpadKey`]s, nie auf rohen User-Strings.
pub trait ScratchpadStore {
    /// Legt einen Wert unter dem genamespacten Schlüssel ab.
    fn put(&self, key: &ScratchpadKey, value: &str);

    /// Liest einen Wert, falls vorhanden.
    fn get(&self, key: &ScratchpadKey) -> Option<String>;

    /// Entfernt einen Wert und meldet, ob etwas entfernt wurde.
    fn delete(&self, key: &ScratchpadKey) -> bool;
}

/// In-Memory-Scratchpad für Tests und lokale Läufe.
#[derive(Default)]
pub struct InMemoryScratchpad {
    data: Mutex<HashMap<String, String>>,
}

impl InMemoryScratchpad {
    /// Erzeugt einen leeren In-Memory-Scratchpad.
    pub fn new() -> Self {
        Self::default()
    }

    /// Anzahl gespeicherter Einträge (nützlich für Tests).
    pub fn len(&self) -> usize {
        self.data.lock().expect("scratchpad mutex poisoned").len()
    }

    /// Ob der Scratchpad leer ist.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ScratchpadStore for InMemoryScratchpad {
    fn put(&self, key: &ScratchpadKey, value: &str) {
        self.data
            .lock()
            .expect("scratchpad mutex poisoned")
            .insert(key.as_str().to_string(), value.to_string());
    }

    fn get(&self, key: &ScratchpadKey) -> Option<String> {
        self.data
            .lock()
            .expect("scratchpad mutex poisoned")
            .get(key.as_str())
            .cloned()
    }

    fn delete(&self, key: &ScratchpadKey) -> bool {
        self.data
            .lock()
            .expect("scratchpad mutex poisoned")
            .remove(key.as_str())
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::{SessionId, TenantContext, TenantId};

    fn ctx(t: &str, s: &str) -> TenantContext {
        TenantContext::new(
            TenantId::from_claim(t).unwrap(),
            SessionId::from_claim(s).unwrap(),
        )
    }

    #[test]
    fn put_then_get_within_same_context() {
        let store = InMemoryScratchpad::new();
        let key = ctx("a", "s1").key("notiz").unwrap();
        store.put(&key, "Zwischenergebnis");
        assert_eq!(store.get(&key).as_deref(), Some("Zwischenergebnis"));
    }

    #[test]
    fn delete_removes_value() {
        let store = InMemoryScratchpad::new();
        let key = ctx("a", "s1").key("notiz").unwrap();
        store.put(&key, "x");
        assert!(store.delete(&key));
        assert_eq!(store.get(&key), None);
        assert!(!store.delete(&key));
    }
}
