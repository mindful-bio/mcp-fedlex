//! Event-Consumer (ADR-003, Entscheidung 2).
//!
//! Konsumiert New-Release-Events des Brokers (Kafka/NATS, hier als Eingabe-Slice
//! abstrahiert). Drei Eigenschaften sind nicht verhandelbar.
//! 1. Dedup. Ein bereits verarbeitetes Release (gleiche `release_id`) wird
//!    übersprungen, At-least-once-Zustellung erzeugt so keine Duplikate.
//! 2. Begrenzte Retries. Ein scheiterndes Release wird N-mal versucht, danach
//!    wandert es mit vollem Kontext in die DLQ. Kein Endlos-Retry.
//! 3. Kein Blockieren. Ein Poison-Release hält den Strom nicht an, nachfolgende
//!    valide Releases werden weiter verarbeitet.

use std::collections::BTreeSet;

use crate::dlq::{DeadLetter, DeadLetterQueue};
use crate::outbox::VersionKey;
use crate::writer::{CacheInvalidator, CorpusSink, IndexWriter};

/// Ein rohes Release-Event vom Broker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEvent {
    /// Stabile, eindeutige Kennung des Releases (Dedup-Schlüssel).
    pub release_id: String,
    /// Rohnutzlast (AKN/Jolux). Wird vom Parser verarbeitet.
    pub raw: String,
}

/// Geparstes, schreibfertiges Release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRelease {
    /// Schlüssel der Norm-Fassung.
    pub key: VersionKey,
    /// Destillierter Text.
    pub text: String,
}

/// Fehler beim Parsen eines Releases.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("parse failure: {0}")]
pub struct ParseError(pub String);

/// Parser gigabytegrosser AKN/Jolux-Releases (hier als Trait abstrahiert).
pub trait ReleaseParser: Send + Sync {
    /// Parst die Rohnutzlast. Fehler markiert ein Poison-Release.
    fn parse(&self, event: &ReleaseEvent) -> Result<ParsedRelease, ParseError>;
}

/// Ergebnis eines Konsum-Laufs (für Metrik und Inspektion).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConsumeReport {
    /// Erfolgreich verarbeitete Releases.
    pub processed: usize,
    /// Als Duplikat übersprungene Releases.
    pub deduped: usize,
    /// In die DLQ verschobene Poison-Releases.
    pub dead_lettered: usize,
}

/// Event-Consumer mit Dedup, begrenzten Retries und DLQ.
pub struct EventConsumer<'a, P, C, V>
where
    P: ReleaseParser,
    C: CorpusSink,
    V: CacheInvalidator,
{
    parser: &'a P,
    writer: &'a IndexWriter<'a, C, V>,
    dlq: &'a DeadLetterQueue,
    max_attempts: u32,
    seen: BTreeSet<String>,
}

impl<'a, P, C, V> EventConsumer<'a, P, C, V>
where
    P: ReleaseParser,
    C: CorpusSink,
    V: CacheInvalidator,
{
    /// Erzeugt den Consumer mit Parser, Writer, DLQ und Retry-Obergrenze.
    pub fn new(
        parser: &'a P,
        writer: &'a IndexWriter<'a, C, V>,
        dlq: &'a DeadLetterQueue,
        max_attempts: u32,
    ) -> Self {
        Self {
            parser,
            writer,
            dlq,
            max_attempts: max_attempts.max(1),
            seen: BTreeSet::new(),
        }
    }

    /// Verarbeitet einen Strom von Events der Reihe nach.
    ///
    /// Ein Poison-Release wird nach `max_attempts` erfolglosen Versuchen in die
    /// DLQ verschoben und hält den Strom nicht an.
    pub fn consume(&mut self, events: &[ReleaseEvent]) -> ConsumeReport {
        let mut report = ConsumeReport::default();
        for event in events {
            // Dedup. Bereits gesehene Release-ID überspringen.
            if self.seen.contains(&event.release_id) {
                report.deduped += 1;
                continue;
            }

            match self.process_with_retries(event) {
                Ok(()) => {
                    self.seen.insert(event.release_id.clone());
                    report.processed += 1;
                }
                Err((reason, attempts)) => {
                    // Poison-Release in die DLQ, Strom läuft weiter.
                    self.dlq.push(DeadLetter {
                        release_id: event.release_id.clone(),
                        raw: event.raw.clone(),
                        reason,
                        attempts,
                    });
                    // Auch ein totes Release gilt als gesehen, kein erneuter Versuch.
                    self.seen.insert(event.release_id.clone());
                    report.dead_lettered += 1;
                }
            }
        }
        report
    }

    /// Versucht ein Release bis zu `max_attempts` mal. Fehler trägt Grund + Zähler.
    ///
    /// Sowohl ein Parse-Fehler als auch ein gescheiterter Korpus-Write zählen
    /// als Fehlversuch. So führt ein vorübergehender Store-Ausfall zu erneuten
    /// Versuchen und im Zweifel zur DLQ, statt still verloren zu gehen.
    fn process_with_retries(&self, event: &ReleaseEvent) -> Result<(), (String, u32)> {
        let mut last_reason = String::new();
        for attempt in 1..=self.max_attempts {
            match self.parser.parse(event) {
                Ok(parsed) => match self.writer.write(parsed.key, &parsed.text) {
                    Ok(_) => return Ok(()),
                    Err(e) => last_reason = format!("attempt {attempt}: {e}"),
                },
                Err(e) => last_reason = format!("attempt {attempt}: {e}"),
            }
        }
        Err((last_reason, self.max_attempts))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbox::EmbeddingOutbox;
    use crate::writer::{InMemoryCorpus, RecordingInvalidator};

    /// Parser, der Releases mit "poison" in der Nutzlast scheitern lässt.
    struct PoisonAwareParser;

    impl ReleaseParser for PoisonAwareParser {
        fn parse(&self, event: &ReleaseEvent) -> Result<ParsedRelease, ParseError> {
            if event.raw.contains("poison") {
                return Err(ParseError("schema violation".into()));
            }
            // Format "eli|version|text".
            let parts: Vec<&str> = event.raw.splitn(3, '|').collect();
            if parts.len() != 3 {
                return Err(ParseError("malformed payload".into()));
            }
            Ok(ParsedRelease {
                key: VersionKey::new(parts[0], parts[1]),
                text: parts[2].to_string(),
            })
        }
    }

    fn valid(id: &str, eli: &str) -> ReleaseEvent {
        ReleaseEvent {
            release_id: id.into(),
            raw: format!("{eli}|2020-01-01|Text"),
        }
    }

    fn poison(id: &str) -> ReleaseEvent {
        ReleaseEvent {
            release_id: id.into(),
            raw: "poison".into(),
        }
    }

    #[test]
    fn poison_release_lands_in_dlq_and_does_not_block_valid_ones() {
        let corpus = InMemoryCorpus::new();
        let outbox = EmbeddingOutbox::new();
        let inval = RecordingInvalidator::new();
        let writer = IndexWriter::new(&corpus, &outbox, &inval);
        let dlq = DeadLetterQueue::new();
        let mut consumer = EventConsumer::new(&PoisonAwareParser, &writer, &dlq, 3);

        // Ein Poison-Release mitten im Strom valider Releases.
        let events = vec![valid("r1", "eli/a"), poison("r2"), valid("r3", "eli/b")];
        let report = consumer.consume(&events);

        // Beide validen Releases sind durch, das Poison-Release blockiert nicht.
        assert_eq!(report.processed, 2);
        assert_eq!(report.dead_lettered, 1);
        assert_eq!(corpus.len(), 2);

        // Das Poison-Release liegt mit vollem Kontext in der DLQ.
        assert_eq!(dlq.depth(), 1);
        let dead = &dlq.entries()[0];
        assert_eq!(dead.release_id, "r2");
        assert_eq!(dead.attempts, 3);
        assert!(dead.reason.contains("schema violation"));
    }

    #[test]
    fn duplicate_release_is_deduped() {
        let corpus = InMemoryCorpus::new();
        let outbox = EmbeddingOutbox::new();
        let inval = RecordingInvalidator::new();
        let writer = IndexWriter::new(&corpus, &outbox, &inval);
        let dlq = DeadLetterQueue::new();
        let mut consumer = EventConsumer::new(&PoisonAwareParser, &writer, &dlq, 3);

        // Dieselbe Release-ID zweimal (At-least-once-Zustellung).
        let events = vec![valid("r1", "eli/a"), valid("r1", "eli/a")];
        let report = consumer.consume(&events);

        assert_eq!(report.processed, 1);
        assert_eq!(report.deduped, 1);
        // Nur ein Korpus-Write trotz doppeltem Event.
        assert_eq!(corpus.len(), 1);
    }

    #[test]
    fn dead_lettered_release_is_not_retried_endlessly() {
        let corpus = InMemoryCorpus::new();
        let outbox = EmbeddingOutbox::new();
        let inval = RecordingInvalidator::new();
        let writer = IndexWriter::new(&corpus, &outbox, &inval);
        let dlq = DeadLetterQueue::new();
        let mut consumer = EventConsumer::new(&PoisonAwareParser, &writer, &dlq, 2);

        // Dasselbe Poison-Release zweimal im Strom. Es darf nur einmal in die DLQ.
        let events = vec![poison("bad"), poison("bad")];
        let report = consumer.consume(&events);
        assert_eq!(report.dead_lettered, 1);
        assert_eq!(report.deduped, 1);
        assert_eq!(dlq.depth(), 1);
    }
}
