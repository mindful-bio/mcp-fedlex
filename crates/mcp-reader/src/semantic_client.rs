//! Semantic-Client. Dünner, optionaler Adapter zum Dienst semantic-fedlex.
//!
//! Zwei Eigenschaften sind nicht verhandelbar.
//! 1. Graceful Degradation. Fällt der semantische Dienst aus, antwortet der
//!    Reader weiter, nur eben rein strukturell. Eine semantische Suche liefert
//!    dann ein leeres, als degradiert markiertes Ergebnis statt eines Fehlers.
//! 2. Provenance je Treffer (ADR-004). Jeder einzelne Treffer trägt seine
//!    eigene [`Provenance`] (ELI + Stichtag). Es gibt keinen Treffer ohne
//!    Herkunft, das Mapping erzwingt das strukturell.

use fedlex_core::{Eli, Provenance, TransactionTime, ValidAsOf};
use time::Date;

/// Roh-Treffer, wie ihn der semantische Dienst liefert (vor dem Mapping).
#[derive(Debug, Clone, PartialEq)]
pub struct RawHit {
    /// ELI des getroffenen Dokuments.
    pub eli: String,
    /// Ähnlichkeitsscore, höher ist besser.
    pub score: f32,
    /// Kurzer Auszug zur Anzeige.
    pub snippet: String,
}

/// Ein gemappter Treffer mit eigener Provenance. Ohne Provenance nicht baubar.
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredHit {
    provenance: Provenance,
    score: f32,
    snippet: String,
}

impl ScoredHit {
    /// Herkunft dieses Treffers.
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// Ähnlichkeitsscore.
    pub fn score(&self) -> f32 {
        self.score
    }

    /// Anzeige-Auszug.
    pub fn snippet(&self) -> &str {
        &self.snippet
    }
}

/// Ergebnis einer semantischen Suche. Trägt das Degradations-Flag.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchOutcome {
    /// Die gemappten Treffer (leer, wenn degradiert).
    pub hits: Vec<ScoredHit>,
    /// Ob der semantische Dienst ausgefallen war und strukturell degradiert wurde.
    pub degraded: bool,
}

impl SearchOutcome {
    /// Ergebnis im Degradationsfall. Leer und als degradiert markiert.
    fn degraded() -> Self {
        Self {
            hits: Vec::new(),
            degraded: true,
        }
    }
}

/// Fehler des Backends. Signalisiert dem Client einen Dienstausfall.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("semantic backend failure: {0}")]
pub struct BackendError(pub String);

/// Backend des semantischen Dienstes. Als Trait für Tests und Austauschbarkeit.
#[async_trait::async_trait]
pub trait SemanticBackend: Send + Sync {
    /// Sucht semantisch. Fehler bedeutet Dienstausfall (löst Degradation aus).
    async fn search(
        &self,
        query: &str,
        as_of: Date,
        top_k: usize,
    ) -> Result<Vec<RawHit>, BackendError>;
}

/// Dünner Client mit Graceful Degradation und Provenance-Mapping.
pub struct SemanticClient<B: SemanticBackend> {
    backend: B,
}

impl<B: SemanticBackend> SemanticClient<B> {
    /// Erzeugt den Client über einem Backend.
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    /// Semantische Suche. Bei Dienstausfall degradiert statt Fehler.
    ///
    /// Jeder Treffer wird mit dem angefragten Stichtag zu einer eigenen
    /// Provenance verheiratet. Treffer mit ungültiger ELI werden still
    /// verworfen, statt das Gesamtergebnis zu kippen.
    pub async fn search(&self, query: &str, as_of: Date, top_k: usize) -> SearchOutcome {
        let raw = match self.backend.search(query, as_of, top_k).await {
            Ok(raw) => raw,
            // Dienst weg. Strukturell degradieren, der Reader bleibt antwortfähig.
            Err(_outage) => return SearchOutcome::degraded(),
        };

        let valid_as_of = ValidAsOf::new(as_of);
        // Eine Systemzeit für den ganzen Lauf, damit alle Treffer derselben
        // Erfassung zugeordnet sind.
        let transaction_time = TransactionTime::now();
        let hits = raw
            .into_iter()
            .filter_map(|hit| {
                let eli = Eli::new(hit.eli.as_str()).ok()?;
                Some(ScoredHit {
                    provenance: Provenance::new(eli, valid_as_of, transaction_time),
                    score: hit.score,
                    snippet: hit.snippet,
                })
            })
            .collect();

        SearchOutcome {
            hits,
            degraded: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Month;

    fn stichtag() -> Date {
        Date::from_calendar_date(2020, Month::January, 1).unwrap()
    }

    struct OkBackend;

    #[async_trait::async_trait]
    impl SemanticBackend for OkBackend {
        async fn search(
            &self,
            _query: &str,
            _as_of: Date,
            _top_k: usize,
        ) -> Result<Vec<RawHit>, BackendError> {
            Ok(vec![
                RawHit {
                    eli: "eli/cc/1999/404".into(),
                    score: 0.91,
                    snippet: "Würde des Menschen".into(),
                },
                RawHit {
                    eli: "eli/cc/2002/61".into(),
                    score: 0.74,
                    snippet: "Treu und Glauben".into(),
                },
            ])
        }
    }

    struct DeadBackend;

    #[async_trait::async_trait]
    impl SemanticBackend for DeadBackend {
        async fn search(
            &self,
            _query: &str,
            _as_of: Date,
            _top_k: usize,
        ) -> Result<Vec<RawHit>, BackendError> {
            Err(BackendError("connection refused".into()))
        }
    }

    struct DirtyBackend;

    #[async_trait::async_trait]
    impl SemanticBackend for DirtyBackend {
        async fn search(
            &self,
            _query: &str,
            _as_of: Date,
            _top_k: usize,
        ) -> Result<Vec<RawHit>, BackendError> {
            Ok(vec![
                RawHit {
                    eli: "eli/cc/1999/404".into(),
                    score: 0.5,
                    snippet: "gut".into(),
                },
                RawHit {
                    // Ungültige ELI (falsches Präfix), muss still verworfen werden.
                    eli: "nonsense/123".into(),
                    score: 0.99,
                    snippet: "boese".into(),
                },
            ])
        }
    }

    #[tokio::test]
    async fn each_hit_carries_its_own_provenance() {
        let client = SemanticClient::new(OkBackend);
        let out = client.search("menschenwürde", stichtag(), 5).await;
        assert!(!out.degraded);
        assert_eq!(out.hits.len(), 2);
        // Jeder Treffer hat eine eigene, korrekt verdrahtete Provenance.
        assert_eq!(out.hits[0].provenance().eli.as_str(), "eli/cc/1999/404");
        assert_eq!(out.hits[1].provenance().eli.as_str(), "eli/cc/2002/61");
        for hit in &out.hits {
            assert_eq!(hit.provenance().valid_as_of.0, stichtag());
        }
    }

    #[tokio::test]
    async fn outage_degrades_gracefully_without_error() {
        let client = SemanticClient::new(DeadBackend);
        let out = client.search("anything", stichtag(), 5).await;
        // Kein Fehler, der Reader bleibt antwortfähig, nur strukturell degradiert.
        assert!(out.degraded);
        assert!(out.hits.is_empty());
    }

    #[tokio::test]
    async fn invalid_eli_hit_is_dropped_not_fatal() {
        let client = SemanticClient::new(DirtyBackend);
        let out = client.search("x", stichtag(), 5).await;
        assert!(!out.degraded);
        // Nur der gültige Treffer bleibt, der mit kaputter ELI fällt heraus.
        assert_eq!(out.hits.len(), 1);
        assert_eq!(out.hits[0].provenance().eli.as_str(), "eli/cc/1999/404");
    }
}
