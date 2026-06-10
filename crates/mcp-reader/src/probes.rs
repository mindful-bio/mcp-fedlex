//! Readiness-Proben für die beiden externen Abhängigkeiten des Readers.
//!
//! `readyz` fragt diese Proben der Reihe nach ab (siehe [`crate::health`]).
//! Beide Proben nutzen bereits vorhandene Clients, statt eigene
//! HTTP-Maschinerie aufzubauen. Die Redis-Probe bucht ein Token aus einem
//! grosszügigen Health-Bucket ab, die Fedlex-Probe stellt eine triviale
//! SPARQL-Anfrage.

use crate::health::ReadinessProbe;
use crate::quota::QuotaBackend;
use fedlex_jolux::SparqlClient;
use fedlex_store::token_bucket::BucketParams;
use std::time::{SystemTime, UNIX_EPOCH};

/// Bucket der Redis-Probe. So grosszügig, dass die Probe selbst nie
/// gedrosselt wird. Ein verweigertes Token wäre trotzdem `ready`, denn die
/// Frage lautet nur, ob Redis antwortet.
const PROBE_PARAMS: BucketParams = BucketParams {
    capacity: 1_000,
    refill_per_sec: 1_000.0,
    ttl_ms: 60_000,
};

/// Prüft das Quota-Backend (Redis) über einen echten Acquire-Roundtrip.
pub struct QuotaBackendProbe<B> {
    backend: B,
}

impl<B> QuotaBackendProbe<B> {
    /// Erstellt die Probe mit einem (geklonten) Backend.
    pub fn new(backend: B) -> Self {
        Self { backend }
    }
}

#[async_trait::async_trait]
impl<B> ReadinessProbe for QuotaBackendProbe<B>
where
    B: QuotaBackend + Send + Sync,
{
    fn name(&self) -> &str {
        "redis"
    }

    async fn ready(&self) -> bool {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        self.backend
            .try_acquire("healthz:probe", PROBE_PARAMS, 1, now_ms)
            .await
            .is_ok()
    }
}

/// Prüft den Fedlex-SPARQL-Endpunkt mit einer leeren SELECT-Anfrage.
pub struct SparqlProbe<C> {
    client: C,
}

impl<C> SparqlProbe<C> {
    /// Erstellt die Probe mit einem eigenen SPARQL-Client.
    pub fn new(client: C) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl<C> ReadinessProbe for SparqlProbe<C>
where
    C: SparqlClient + Send + Sync,
{
    fn name(&self) -> &str {
        "fedlex"
    }

    async fn ready(&self) -> bool {
        self.client
            .query("SELECT (1 AS ?ok) WHERE {} LIMIT 1")
            .await
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::QuotaError;
    use fedlex_jolux::MockSparqlClient;
    use fedlex_store::token_bucket::Acquisition;

    struct FailingBackend;

    impl QuotaBackend for FailingBackend {
        async fn try_acquire(
            &self,
            _key: &str,
            _params: BucketParams,
            _cost: u32,
            _now_ms: u64,
        ) -> Result<Acquisition, QuotaError> {
            Err(QuotaError::Backend("down".into()))
        }
    }

    struct OkBackend;

    impl QuotaBackend for OkBackend {
        async fn try_acquire(
            &self,
            _key: &str,
            _params: BucketParams,
            _cost: u32,
            _now_ms: u64,
        ) -> Result<Acquisition, QuotaError> {
            Ok(Acquisition {
                allowed: true,
                remaining: 1,
                retry_after_ms: 0,
            })
        }
    }

    #[tokio::test]
    async fn quota_probe_reflects_backend_reachability() {
        assert!(QuotaBackendProbe::new(OkBackend).ready().await);
        assert!(!QuotaBackendProbe::new(FailingBackend).ready().await);
    }

    #[tokio::test]
    async fn sparql_probe_is_ready_when_query_succeeds() {
        let client = MockSparqlClient::from_json(
            r#"{ "head": { "vars": ["ok"] }, "results": { "bindings": [] } }"#,
        );
        assert!(SparqlProbe::new(client).ready().await);
    }
}
