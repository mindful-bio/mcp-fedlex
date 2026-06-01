//! Oxigraph-Adapter der Korpus-Senke (Lücke B, Schreib- trifft Lesepfad).
//!
//! Verbindet den Schreibpfad ([`CorpusSink`]) mit dem eingebetteten,
//! bi-temporalen Oxigraph-Korpus der Leseseite. So landet eine geschriebene
//! Fassung im selben Store, aus dem der Reader sie später punkt-in-zeit auflöst.
//!
//! Die Abbildung der Zeitachsen. Die Fassungs-Kennung des [`VersionKey`] ist der
//! Stichtag (`valid_from`), ab dem die Norm rechtlich gilt. Die Transaktionszeit
//! (`transaction_time`) ist der Moment des Schreibens, also wann der Dienst die
//! Fassung erfuhr. Eine spätere Korrektur derselben Gültigkeit gewinnt so über
//! die höhere Transaktionszeit, ohne die Historie zu verlieren.
//!
//! Hinter dem Feature `oxigraph-store`, damit die schwere RocksDB-Abhängigkeit
//! den Standard-Build nicht belastet.

use fedlex_store::OxigraphCorpus;
use time::macros::format_description;
use time::{Date, OffsetDateTime};

use crate::outbox::VersionKey;
use crate::writer::{CorpusError, CorpusSink};

/// Korpus-Senke über dem eingebetteten Oxigraph-Korpus.
///
/// Die Senke ist append-only. Jeder erfolgreiche Append fügt eine Fassung hinzu,
/// nie wird überschrieben. Ein vorübergehender Store-Fehler wird als
/// [`CorpusError`] nach oben gereicht, damit der Consumer erneut versucht.
pub struct OxigraphCorpusSink {
    corpus: OxigraphCorpus,
}

impl OxigraphCorpusSink {
    /// Erzeugt die Senke über einem bestehenden Korpus.
    pub fn new(corpus: OxigraphCorpus) -> Self {
        Self { corpus }
    }

    /// Leiht den darunterliegenden Korpus, etwa für Punkt-in-Zeit-Abfragen im
    /// Test oder beim Cache-Warmup.
    pub fn corpus(&self) -> &OxigraphCorpus {
        &self.corpus
    }

    /// Liest den Stichtag aus der Fassungs-Kennung. Format `[year]-[month]-[day]`.
    fn valid_from(key: &VersionKey) -> Result<Date, CorpusError> {
        let format = format_description!("[year]-[month]-[day]");
        Date::parse(&key.version, &format)
            .map_err(|e| CorpusError(format!("invalid version date '{}': {e}", key.version)))
    }
}

impl CorpusSink for OxigraphCorpusSink {
    fn append(&self, key: &VersionKey, text: &str) -> Result<(), CorpusError> {
        let valid_from = Self::valid_from(key)?;
        let transaction_time = OffsetDateTime::now_utc();
        self.corpus
            .append_version(&key.eli, &key.version, valid_from, transaction_time, text)
            .map_err(|e| CorpusError(e.to_string()))
    }

    fn contains(&self, key: &VersionKey) -> Result<bool, CorpusError> {
        self.corpus
            .contains_version(&key.eli, &key.version)
            .map_err(|e| CorpusError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbox::EmbeddingOutbox;
    use crate::writer::{IndexWriter, RecordingInvalidator};
    use time::macros::date;

    #[test]
    fn write_through_sink_is_resolvable_point_in_time() {
        let sink = OxigraphCorpusSink::new(OxigraphCorpus::new().unwrap());
        let outbox = EmbeddingOutbox::new();
        let inval = RecordingInvalidator::new();
        let writer = IndexWriter::new(&sink, &outbox, &inval);

        let key = VersionKey::new("eli/cc/1999/404", "2020-01-01");
        assert!(writer.write(key.clone(), "Würde des Menschen").unwrap());

        // Der Reader-Pfad löst die Fassung zum Stichtag auf.
        let at = sink
            .corpus()
            .resolve_as_of("eli/cc/1999/404", date!(2021 - 06 - 01));
        assert_eq!(at.unwrap(), Some("Würde des Menschen".to_string()));
        // Vor der Gültigkeit gibt es nichts.
        let before = sink
            .corpus()
            .resolve_as_of("eli/cc/1999/404", date!(2019 - 01 - 01));
        assert_eq!(before.unwrap(), None);
    }

    #[test]
    fn write_is_idempotent_via_contains() {
        let sink = OxigraphCorpusSink::new(OxigraphCorpus::new().unwrap());
        let outbox = EmbeddingOutbox::new();
        let inval = RecordingInvalidator::new();
        let writer = IndexWriter::new(&sink, &outbox, &inval);
        let key = VersionKey::new("eli/a", "2020-01-01");

        assert!(writer.write(key.clone(), "A").unwrap());
        // Zweiter Write derselben Fassung ist ein No-Op, der Korpus zählt eine Fassung.
        assert!(!writer.write(key.clone(), "A").unwrap());
        assert_eq!(sink.corpus().version_count("eli/a").unwrap(), 1);
        // Genau ein Invalidierungs-Event.
        assert_eq!(inval.events().len(), 1);
    }

    #[test]
    fn two_validity_dates_resolve_to_the_right_one() {
        let sink = OxigraphCorpusSink::new(OxigraphCorpus::new().unwrap());
        let outbox = EmbeddingOutbox::new();
        let inval = RecordingInvalidator::new();
        let writer = IndexWriter::new(&sink, &outbox, &inval);

        // Zwei Fassungen derselben Norm mit verschiedenen Stichtagen.
        writer
            .write(VersionKey::new("eli/a", "2020-01-01"), "alte Fassung")
            .unwrap();
        writer
            .write(VersionKey::new("eli/a", "2022-01-01"), "neue Fassung")
            .unwrap();

        // Zwischen den Stichtagen gilt die alte Fassung.
        let mid = sink.corpus().resolve_as_of("eli/a", date!(2021 - 06 - 01));
        assert_eq!(mid.unwrap(), Some("alte Fassung".to_string()));
        // Nach dem zweiten Stichtag gilt die neue Fassung.
        let later = sink.corpus().resolve_as_of("eli/a", date!(2023 - 01 - 01));
        assert_eq!(later.unwrap(), Some("neue Fassung".to_string()));
        // Beide Fassungen liegen im Korpus, nichts ging verloren.
        assert_eq!(sink.corpus().version_count("eli/a").unwrap(), 2);
    }

    #[test]
    fn malformed_version_date_is_a_corpus_error() {
        let sink = OxigraphCorpusSink::new(OxigraphCorpus::new().unwrap());
        let key = VersionKey::new("eli/a", "nicht-ein-datum");
        let err = sink.append(&key, "x").unwrap_err();
        assert!(matches!(err, CorpusError(msg) if msg.contains("invalid version date")));
    }
}
