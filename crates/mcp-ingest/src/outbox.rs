//! Transaktionale Embedding-Outbox (ADR-003, Entscheidung 1).
//!
//! Statt `semantic-fedlex` synchron beim Korpus-Write aufzurufen, wird der
//! Embedding-Auftrag in einer Outbox festgeschrieben (in derselben Transaktion
//! wie der Korpus-Write, kein Verlust dazwischen). Ein separater Zusteller liest
//! die Outbox und ruft `index()` mit exponentiellem Backoff auf. Fällt der
//! GPU-Dienst aus, staut sich der Auftrag sichtbar in der Outbox, statt still zu
//! verschwinden. Nach Recovery zieht der Zusteller nach.
//!
//! Pro `eli:version` führt die Outbox einen Vollständigkeits-Marker
//! (`corpus_written` / `vectors_indexed`). So macht eine Abfrage "Korpus ohne
//! Vektoren" jeden Drift sichtbar und alarmierbar.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

/// Stabiler Schlüssel eines Auftrags. Identität pro Norm-Fassung.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct VersionKey {
    /// ELI der Norm.
    pub eli: String,
    /// Fassungs-Kennung (z.B. Stichtag oder Revision).
    pub version: String,
}

impl VersionKey {
    /// Erzeugt einen Schlüssel aus ELI und Fassung.
    pub fn new(eli: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            eli: eli.into(),
            version: version.into(),
        }
    }
}

/// Vollständigkeits-Marker pro `eli:version`. Macht Drift sichtbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Completeness {
    /// Der Korpus (DOM/Graph) wurde geschrieben.
    pub corpus_written: bool,
    /// Die Vektoren wurden im semantischen Index materialisiert.
    pub vectors_indexed: bool,
}

#[derive(Debug, Clone)]
struct OutboxEntry {
    text: String,
    attempts: u32,
    marker: Completeness,
}

/// Fehler des semantischen Indexers. Signalisiert Dienstausfall.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("semantic index failure: {0}")]
pub struct IndexError(pub String);

/// Senke für Vektoren. Idempotent pro `eli:version` (Retries ohne Duplikate).
#[async_trait::async_trait]
pub trait SemanticIndexer: Send + Sync {
    /// Materialisiert Vektoren. Muss idempotent pro Schlüssel sein.
    async fn index(&self, key: &VersionKey, text: &str) -> Result<(), IndexError>;
}

/// Transaktionale Outbox mit Vollständigkeits-Markern.
#[derive(Debug, Default)]
pub struct EmbeddingOutbox {
    entries: Mutex<BTreeMap<VersionKey, OutboxEntry>>,
}

impl EmbeddingOutbox {
    /// Erzeugt eine leere Outbox.
    pub fn new() -> Self {
        Self::default()
    }

    /// Schreibt einen Auftrag fest und markiert den Korpus als geschrieben.
    ///
    /// Idempotent. Ein erneuter Enqueue desselben Schlüssels überschreibt den
    /// Text, setzt aber einen bereits gesetzten `vectors_indexed`-Marker nicht
    /// zurück.
    pub fn enqueue(&self, key: VersionKey, text: impl Into<String>) {
        let mut map = self.entries.lock().expect("outbox mutex poisoned");
        let entry = map.entry(key).or_insert_with(|| OutboxEntry {
            text: String::new(),
            attempts: 0,
            marker: Completeness::default(),
        });
        entry.text = text.into();
        entry.marker.corpus_written = true;
    }

    /// Anzahl noch nicht indexierter Aufträge (Backlog-Tiefe als Metrik).
    pub fn backlog_depth(&self) -> usize {
        self.entries
            .lock()
            .expect("outbox mutex poisoned")
            .values()
            .filter(|e| !e.marker.vectors_indexed)
            .count()
    }

    /// Schlüssel mit geschriebenem Korpus, aber fehlenden Vektoren (Drift).
    ///
    /// Genau diese Abfrage macht den stillen Index-Drift sichtbar.
    pub fn incomplete(&self) -> Vec<VersionKey> {
        self.entries
            .lock()
            .expect("outbox mutex poisoned")
            .iter()
            .filter(|(_, e)| e.marker.corpus_written && !e.marker.vectors_indexed)
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Marker eines Schlüssels (für Inspektion und Tests).
    pub fn marker(&self, key: &VersionKey) -> Option<Completeness> {
        self.entries
            .lock()
            .expect("outbox mutex poisoned")
            .get(key)
            .map(|e| e.marker)
    }

    fn pending_jobs(&self) -> Vec<(VersionKey, String)> {
        self.entries
            .lock()
            .expect("outbox mutex poisoned")
            .iter()
            .filter(|(_, e)| !e.marker.vectors_indexed)
            .map(|(k, e)| (k.clone(), e.text.clone()))
            .collect()
    }

    fn mark_indexed(&self, key: &VersionKey) {
        if let Some(entry) = self
            .entries
            .lock()
            .expect("outbox mutex poisoned")
            .get_mut(key)
        {
            entry.marker.vectors_indexed = true;
        }
    }

    fn record_attempt(&self, key: &VersionKey) {
        if let Some(entry) = self
            .entries
            .lock()
            .expect("outbox mutex poisoned")
            .get_mut(key)
        {
            entry.attempts += 1;
        }
    }
}

/// Konfiguration des Zustellers.
#[derive(Debug, Clone, Copy)]
pub struct DelivererConfig {
    /// Basis-Wartezeit des exponentiellen Backoffs.
    pub base_backoff: Duration,
    /// Obergrenze einer einzelnen Wartezeit.
    pub max_backoff: Duration,
    /// Maximale Zustellrunden je Aufruf von `deliver_pending`.
    pub max_rounds: u32,
}

impl Default for DelivererConfig {
    fn default() -> Self {
        Self {
            base_backoff: Duration::from_millis(50),
            max_backoff: Duration::from_secs(30),
            max_rounds: 8,
        }
    }
}

/// Idempotenter Zusteller, der die Outbox in den semantischen Index nachzieht.
pub struct OutboxDeliverer<'a, I: SemanticIndexer> {
    outbox: &'a EmbeddingOutbox,
    indexer: &'a I,
    config: DelivererConfig,
}

impl<'a, I: SemanticIndexer> OutboxDeliverer<'a, I> {
    /// Erzeugt einen Zusteller über Outbox und Indexer.
    pub fn new(outbox: &'a EmbeddingOutbox, indexer: &'a I, config: DelivererConfig) -> Self {
        Self {
            outbox,
            indexer,
            config,
        }
    }

    /// Versucht, alle offenen Aufträge zuzustellen, mit Backoff zwischen Runden.
    ///
    /// Bricht ab, sobald nichts mehr offen ist oder `max_rounds` erreicht wurde.
    /// Gibt die Zahl erfolgreich zugestellter Aufträge zurück. Bei anhaltendem
    /// Dienstausfall bleiben die Aufträge in der Outbox (sichtbarer Backlog),
    /// kein stiller Drift.
    pub async fn deliver_pending(&self) -> usize {
        let mut delivered = 0;
        let mut backoff = self.config.base_backoff;

        for round in 0..self.config.max_rounds {
            let pending = self.outbox.pending_jobs();
            if pending.is_empty() {
                break;
            }

            let mut progressed = false;
            for (key, text) in pending {
                self.outbox.record_attempt(&key);
                match self.indexer.index(&key, &text).await {
                    Ok(()) => {
                        self.outbox.mark_indexed(&key);
                        delivered += 1;
                        progressed = true;
                    }
                    Err(_outage) => {}
                }
            }

            // Nichts ging durch und es bleibt offen. Backoff vor der nächsten Runde.
            if !progressed && round + 1 < self.config.max_rounds {
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(self.config.max_backoff);
            }
        }
        delivered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Indexer, der die ersten `fail_until` Aufrufe scheitern lässt (Ausfall),
    /// danach erfolgreich antwortet (Recovery). Zählt Aufrufe je Schlüssel.
    struct FlakyIndexer {
        fail_until: usize,
        calls: AtomicUsize,
        seen: Mutex<BTreeMap<String, usize>>,
    }

    impl FlakyIndexer {
        fn new(fail_until: usize) -> Self {
            Self {
                fail_until,
                calls: AtomicUsize::new(0),
                seen: Mutex::new(BTreeMap::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl SemanticIndexer for FlakyIndexer {
        async fn index(&self, key: &VersionKey, _text: &str) -> Result<(), IndexError> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n < self.fail_until {
                return Err(IndexError("gpu service down".into()));
            }
            // Idempotenz-Beobachtung. Zählt erfolgreiche Indexierungen je Schlüssel.
            *self
                .seen
                .lock()
                .unwrap()
                .entry(format!("{}:{}", key.eli, key.version))
                .or_insert(0) += 1;
            Ok(())
        }
    }

    fn fast_config() -> DelivererConfig {
        DelivererConfig {
            base_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(4),
            max_rounds: 10,
        }
    }

    #[tokio::test]
    async fn outage_fills_outbox_then_recovers_without_silent_drift() {
        let outbox = EmbeddingOutbox::new();
        let key = VersionKey::new("eli/cc/1999/404", "2020-01-01");
        outbox.enqueue(key.clone(), "Würde des Menschen");

        // Korpus geschrieben, Vektoren fehlen. Drift ist sichtbar.
        assert_eq!(outbox.backlog_depth(), 1);
        assert_eq!(outbox.incomplete(), vec![key.clone()]);

        // Dienst scheitert die ersten drei Versuche, dann Recovery.
        let indexer = FlakyIndexer::new(3);
        let deliverer = OutboxDeliverer::new(&outbox, &indexer, fast_config());
        let delivered = deliverer.deliver_pending().await;

        assert_eq!(delivered, 1);
        // Nach Recovery ist der Backlog leer und kein Drift bleibt.
        assert_eq!(outbox.backlog_depth(), 0);
        assert!(outbox.incomplete().is_empty());
        assert_eq!(
            outbox.marker(&key).unwrap(),
            Completeness {
                corpus_written: true,
                vectors_indexed: true
            }
        );
        // Idempotenz. Genau eine erfolgreiche Indexierung trotz Retries.
        assert_eq!(
            indexer.seen.lock().unwrap()["eli/cc/1999/404:2020-01-01"],
            1
        );
    }

    #[tokio::test]
    async fn permanent_outage_keeps_backlog_visible() {
        let outbox = EmbeddingOutbox::new();
        outbox.enqueue(VersionKey::new("eli/x", "v1"), "t");

        // Dienst bleibt unten (fail_until sehr hoch).
        let indexer = FlakyIndexer::new(usize::MAX);
        let deliverer = OutboxDeliverer::new(&outbox, &indexer, fast_config());
        let delivered = deliverer.deliver_pending().await;

        assert_eq!(delivered, 0);
        // Kein stiller Verlust. Der Auftrag bleibt sichtbar im Backlog.
        assert_eq!(outbox.backlog_depth(), 1);
        assert_eq!(outbox.incomplete().len(), 1);
    }

    #[tokio::test]
    async fn enqueue_is_idempotent_and_preserves_indexed_marker() {
        let outbox = EmbeddingOutbox::new();
        let key = VersionKey::new("eli/y", "v2");
        outbox.enqueue(key.clone(), "alt");

        let indexer = FlakyIndexer::new(0);
        OutboxDeliverer::new(&outbox, &indexer, fast_config())
            .deliver_pending()
            .await;
        assert!(outbox.marker(&key).unwrap().vectors_indexed);

        // Erneuter Enqueue darf den Indexed-Marker nicht zurücksetzen.
        outbox.enqueue(key.clone(), "neu");
        assert!(outbox.marker(&key).unwrap().vectors_indexed);
    }
}
