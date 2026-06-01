//! Index-Writer. Idempotenter, append-only Korpus-Write plus Outbox-Enqueue und
//! Cache-Invalidierung.
//!
//! Der Korpus wird append-only und bi-temporal geschrieben (Platzhalter für die
//! Oxigraph-/Redis-Senken, hier als Trait abstrahiert). In derselben logischen
//! Transaktion wird der Embedding-Auftrag in die Outbox geschrieben (ADR-003,
//! kein Verlust zwischen Write und Enqueue). Nach jedem erfolgreichen Write geht
//! ein Cache-Invalidierungs-Event hinaus, damit die Reader-Pods ihren L1-Cache
//! verwerfen.

use std::sync::Mutex;

use crate::outbox::{EmbeddingOutbox, VersionKey};

/// Fehler einer Korpus-Senke. Ein realer Store (Oxigraph/Redis) kann scheitern.
/// Der Fehler wird nicht verschluckt, sondern an den Consumer gereicht, der
/// erneut versucht und im Zweifel in die DLQ verschiebt.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("corpus sink failure: {0}")]
pub struct CorpusError(pub String);

/// Append-only, bi-temporale Korpus-Senke (Oxigraph/Redis-Abstraktion).
pub trait CorpusSink: Send + Sync {
    /// Hängt eine Norm-Fassung an. Append-only, nie destruktiv.
    fn append(&self, key: &VersionKey, text: &str) -> Result<(), CorpusError>;
    /// Ob eine Fassung bereits geschrieben wurde (Idempotenz-Prüfung).
    fn contains(&self, key: &VersionKey) -> Result<bool, CorpusError>;
}

/// Empfänger von Cache-Invalidierungs-Events.
pub trait CacheInvalidator: Send + Sync {
    /// Signalisiert, dass die Fassung neu geschrieben wurde.
    fn invalidate(&self, key: &VersionKey);
}

/// In-Memory-Korpus für Tests und lokale Läufe. Append-only.
#[derive(Debug, Default)]
pub struct InMemoryCorpus {
    written: Mutex<Vec<VersionKey>>,
}

impl InMemoryCorpus {
    /// Erzeugt einen leeren Korpus.
    pub fn new() -> Self {
        Self::default()
    }

    /// Anzahl geschriebener Fassungen.
    pub fn len(&self) -> usize {
        self.written.lock().expect("corpus mutex poisoned").len()
    }

    /// Ob der Korpus leer ist.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl CorpusSink for InMemoryCorpus {
    fn append(&self, key: &VersionKey, _text: &str) -> Result<(), CorpusError> {
        self.written
            .lock()
            .expect("corpus mutex poisoned")
            .push(key.clone());
        Ok(())
    }

    fn contains(&self, key: &VersionKey) -> Result<bool, CorpusError> {
        Ok(self
            .written
            .lock()
            .expect("corpus mutex poisoned")
            .contains(key))
    }
}

/// Sammelt Invalidierungs-Events. Für Tests und lokale Läufe.
#[derive(Debug, Default)]
pub struct RecordingInvalidator {
    events: Mutex<Vec<VersionKey>>,
}

impl RecordingInvalidator {
    /// Erzeugt einen leeren Recorder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Schnappschuss der ausgelösten Events.
    pub fn events(&self) -> Vec<VersionKey> {
        self.events
            .lock()
            .expect("invalidator mutex poisoned")
            .clone()
    }
}

impl CacheInvalidator for RecordingInvalidator {
    fn invalidate(&self, key: &VersionKey) {
        self.events
            .lock()
            .expect("invalidator mutex poisoned")
            .push(key.clone());
    }
}

/// Schreibt Korpus und Outbox-Auftrag idempotent und invalidiert den Cache.
pub struct IndexWriter<'a, C: CorpusSink, V: CacheInvalidator> {
    corpus: &'a C,
    outbox: &'a EmbeddingOutbox,
    invalidator: &'a V,
}

impl<'a, C: CorpusSink, V: CacheInvalidator> IndexWriter<'a, C, V> {
    /// Erzeugt den Writer über Korpus, Outbox und Invalidator.
    pub fn new(corpus: &'a C, outbox: &'a EmbeddingOutbox, invalidator: &'a V) -> Self {
        Self {
            corpus,
            outbox,
            invalidator,
        }
    }

    /// Schreibt eine Fassung. Idempotent pro `eli:version`.
    ///
    /// Eine bereits geschriebene Fassung wird übersprungen (kein Duplikat, kein
    /// erneutes Event). Sonst Korpus-Append, Outbox-Enqueue und ein
    /// Invalidierungs-Event. Gibt zurück, ob tatsächlich geschrieben wurde.
    /// Scheitert die Korpus-Senke, wird der Fehler nach oben gereicht, bevor
    /// Outbox oder Invalidierung laufen (kein Drift bei halbem Write).
    pub fn write(&self, key: VersionKey, text: &str) -> Result<bool, CorpusError> {
        if self.corpus.contains(&key)? {
            return Ok(false);
        }
        self.corpus.append(&key, text)?;
        self.outbox.enqueue(key.clone(), text);
        self.invalidator.invalidate(&key);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_is_idempotent_per_version() {
        let corpus = InMemoryCorpus::new();
        let outbox = EmbeddingOutbox::new();
        let inval = RecordingInvalidator::new();
        let writer = IndexWriter::new(&corpus, &outbox, &inval);
        let key = VersionKey::new("eli/cc/1999/404", "2020-01-01");

        assert!(writer.write(key.clone(), "t").unwrap());
        // Zweiter Write derselben Fassung ist ein No-Op.
        assert!(!writer.write(key.clone(), "t").unwrap());

        assert_eq!(corpus.len(), 1);
        // Genau ein Invalidierungs-Event, nicht zwei.
        assert_eq!(inval.events().len(), 1);
        // Outbox trägt den Auftrag mit gesetztem Korpus-Marker.
        assert!(outbox.marker(&key).unwrap().corpus_written);
    }

    #[test]
    fn write_emits_invalidation_after_each_new_version() {
        let corpus = InMemoryCorpus::new();
        let outbox = EmbeddingOutbox::new();
        let inval = RecordingInvalidator::new();
        let writer = IndexWriter::new(&corpus, &outbox, &inval);

        writer.write(VersionKey::new("eli/a", "v1"), "a").unwrap();
        writer.write(VersionKey::new("eli/a", "v2"), "b").unwrap();
        assert_eq!(inval.events().len(), 2);
    }

    /// Korpus-Senke, die jeden Append scheitern lässt.
    struct FailingCorpus;

    impl CorpusSink for FailingCorpus {
        fn append(&self, _key: &VersionKey, _text: &str) -> Result<(), CorpusError> {
            Err(CorpusError("store unreachable".into()))
        }
        fn contains(&self, _key: &VersionKey) -> Result<bool, CorpusError> {
            Ok(false)
        }
    }

    #[test]
    fn write_failure_propagates_without_outbox_or_invalidation() {
        let corpus = FailingCorpus;
        let outbox = EmbeddingOutbox::new();
        let inval = RecordingInvalidator::new();
        let writer = IndexWriter::new(&corpus, &outbox, &inval);
        let key = VersionKey::new("eli/a", "v1");

        let err = writer.write(key.clone(), "t").unwrap_err();
        assert_eq!(err, CorpusError("store unreachable".into()));
        // Kein Outbox-Auftrag und kein Invalidierungs-Event bei gescheitertem Write.
        assert!(outbox.marker(&key).is_none());
        assert!(inval.events().is_empty());
    }
}
