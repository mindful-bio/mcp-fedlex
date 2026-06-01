//! fedlex-store - Data-Access-Layer mit Tenant-Isolation (ADR-001).
//!
//! Der Kern ist die Tenant-Isolation des Agent-Scratchpads. Sie steckt im
//! Schlüssel ([`key`]) und ist damit unabhängig vom Speicher beweisbar. Das
//! [`TenantRepository`] ist die einzige erlaubte Schnittstelle und injiziert
//! den Namespace `tenant:session:key`, der [`ScratchpadStore`] ist austauschbar
//! (in-memory für Tests, Redis in Produktion).

#![forbid(unsafe_code)]

pub mod key;
pub mod repository;
pub mod scratchpad;

#[cfg(feature = "redis-store")]
pub mod redis_store;

#[cfg(feature = "redis-store")]
pub mod token_bucket;

#[cfg(feature = "oxigraph-store")]
pub mod oxigraph_corpus;

pub use key::{KeyError, ScratchpadKey, SessionId, TenantContext, TenantId};
pub use repository::TenantRepository;
pub use scratchpad::{InMemoryScratchpad, ScratchpadStore};

#[cfg(feature = "redis-store")]
pub use redis_store::{RedisError, RedisScratchpad};

#[cfg(feature = "redis-store")]
pub use token_bucket::{Acquisition, BucketParams, RedisTokenBucket};

#[cfg(feature = "oxigraph-store")]
pub use oxigraph_corpus::{GraphError, OxigraphCorpus};
