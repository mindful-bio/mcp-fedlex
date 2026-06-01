//! mcp-ingest - event-getriebener Writer (CQRS-Schreibseite).
//!
//! Ab M8 der resiliente Schreibpfad nach ADR-003. Der [`consumer`] konsumiert
//! Release-Events mit Dedup und schiebt Poison-Releases nach begrenzten Retries
//! in die [`dlq`], ohne den Strom zu blockieren. Der [`writer`] schreibt den
//! Korpus idempotent und append-only, stellt den Embedding-Auftrag transaktional
//! in die [`outbox`] und löst ein Cache-Invalidierungs-Event aus. Ein Zusteller
//! zieht die Outbox mit Backoff in den semantischen Index nach, sodass ein
//! Dienstausfall sichtbaren Backlog erzeugt statt stillem Drift. Hinter dem
//! Feature `oxigraph-store` verdrahtet der [`oxigraph_sink`] den Schreibpfad an
//! den eingebetteten, bi-temporalen Oxigraph-Korpus der Leseseite.

#![forbid(unsafe_code)]

pub mod consumer;
pub mod dlq;
pub mod outbox;
#[cfg(feature = "oxigraph-store")]
pub mod oxigraph_sink;
pub mod writer;

pub use consumer::{
    ConsumeReport, EventConsumer, ParseError, ParsedRelease, ReleaseEvent, ReleaseParser,
};
pub use dlq::{DeadLetter, DeadLetterQueue};
pub use outbox::{
    Completeness, DelivererConfig, EmbeddingOutbox, IndexError, OutboxDeliverer, SemanticIndexer,
    VersionKey,
};
#[cfg(feature = "oxigraph-store")]
pub use oxigraph_sink::OxigraphCorpusSink;
pub use writer::{
    CacheInvalidator, CorpusError, CorpusSink, InMemoryCorpus, IndexWriter, RecordingInvalidator,
};
