//! Verteiltes Quota mit Fail-closed-Fallback (ADR-002).
//!
//! Das Token-Bucket liegt verteilt in Redis ([`fedlex_store`]). Diese Schicht
//! bindet das Limit an den server-validierten Claim. Der Bucket-Schlüssel wird
//! ausschliesslich aus Mandant und Session des Claims gebildet, niemals aus
//! Tool-Argumenten. Eine LLM-Halluzination kann das Quota also weder umgehen
//! noch auf einen fremden Mandanten umlenken.
//!
//! Fällt Redis aus, wird NICHT freigeschaltet. Stattdessen greift ein
//! konservatives pod-lokales Limit (Fail-closed), das im degradierten Betrieb
//! deutlich strenger drosselt.

use crate::auth::{Role, VerifiedClaims};
use fedlex_store::token_bucket::{Acquisition, BucketParams};
use std::collections::HashMap;
use std::sync::Mutex;

/// Fehler des Quota-Backends.
#[derive(Debug, thiserror::Error)]
pub enum QuotaError {
    /// Das verteilte Backend war nicht erreichbar.
    #[error("quota backend unavailable: {0}")]
    Backend(String),
}

/// Abstraktion über das verteilte Token-Bucket. Im Test mockbar.
pub trait QuotaBackend {
    /// Versucht atomar, `cost` Tokens aus dem Bucket `key` abzubuchen.
    fn try_acquire(
        &self,
        key: &str,
        params: BucketParams,
        cost: u32,
        now_ms: u64,
    ) -> impl std::future::Future<Output = Result<Acquisition, QuotaError>> + Send;
}

pub use redis_backend::RedisQuotaBackend;

mod redis_backend {
    use super::*;
    use fedlex_store::RedisTokenBucket;

    /// Redis-gestütztes Quota-Backend (Produktion).
    #[derive(Clone)]
    pub struct RedisQuotaBackend {
        bucket: RedisTokenBucket,
    }

    impl RedisQuotaBackend {
        /// Verbindet sich mit einer Redis-URL.
        pub fn connect(url: &str) -> Result<Self, QuotaError> {
            let bucket =
                RedisTokenBucket::connect(url).map_err(|e| QuotaError::Backend(e.to_string()))?;
            Ok(Self { bucket })
        }
    }

    impl QuotaBackend for RedisQuotaBackend {
        async fn try_acquire(
            &self,
            key: &str,
            params: BucketParams,
            cost: u32,
            now_ms: u64,
        ) -> Result<Acquisition, QuotaError> {
            self.bucket
                .try_acquire(key, params, cost, now_ms)
                .await
                .map_err(|e| QuotaError::Backend(e.to_string()))
        }
    }
}

/// Quota-Politik. Mappt Rolle auf Bucket-Parameter und definiert das
/// konservative Fallback-Limit.
#[derive(Debug, Clone, Copy)]
pub struct QuotaPolicy {
    ttl_ms: u64,
}

impl Default for QuotaPolicy {
    fn default() -> Self {
        Self { ttl_ms: 60_000 }
    }
}

impl QuotaPolicy {
    /// Bucket-Parameter pro Rolle. Höhere Rollen erhalten grössere Buckets.
    pub fn params_for(&self, role: Role) -> BucketParams {
        let (capacity, refill_per_sec) = match role {
            Role::Reader => (60, 1.0),
            Role::Navigator => (120, 2.0),
            Role::Validator => (240, 4.0),
        };
        BucketParams {
            capacity,
            refill_per_sec,
            ttl_ms: self.ttl_ms,
        }
    }

    /// Konservatives pod-lokales Limit im degradierten Betrieb (Redis-Ausfall).
    /// Bewusst streng, damit ein Ausfall nicht zur Quota-Umgehung wird.
    pub fn fallback_params(&self) -> BucketParams {
        BucketParams {
            capacity: 5,
            refill_per_sec: 0.5,
            ttl_ms: self.ttl_ms,
        }
    }
}

/// Bildet den Bucket-Schlüssel ausschliesslich aus dem validierten Claim.
/// Mandant und Session enthalten garantiert kein `:` (Schlüssel-Invariante),
/// der Namespace ist damit eindeutig und nicht aus LLM-Eingaben formbar.
fn quota_key(claims: &VerifiedClaims) -> String {
    format!(
        "quota:{}:{}",
        claims.tenant().as_str(),
        claims.session().as_str()
    )
}

/// Entscheidung über einen Tool-Aufruf.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decision {
    /// Ob der Aufruf erlaubt ist.
    pub allowed: bool,
    /// Verbleibende Tokens.
    pub remaining: i64,
    /// Wartezeit bis zur nächsten Erlaubnis in Millisekunden.
    pub retry_after_ms: i64,
    /// Ob im degradierten Fallback-Modus (Redis-Ausfall) entschieden wurde.
    pub degraded: bool,
}

/// Pod-lokales In-Memory-Token-Bucket. Nur als konservativer Fallback.
#[derive(Debug, Default)]
struct PodLocalBucket {
    state: Mutex<HashMap<String, (f64, u64)>>,
}

impl PodLocalBucket {
    fn try_acquire(&self, key: &str, params: BucketParams, cost: u32, now_ms: u64) -> Acquisition {
        let mut state = self.state.lock().expect("pod-local bucket poisoned");
        let (tokens, ts) = state
            .get(key)
            .copied()
            .unwrap_or((params.capacity as f64, now_ms));

        let elapsed = now_ms.saturating_sub(ts) as f64;
        let mut available =
            (tokens + (elapsed / 1000.0) * params.refill_per_sec).min(params.capacity as f64);

        let cost = cost as f64;
        let allowed = available >= cost;
        if allowed {
            available -= cost;
        }
        state.insert(key.to_string(), (available, now_ms));

        let retry_after_ms = if allowed {
            0
        } else if params.refill_per_sec > 0.0 {
            (((cost - available) / params.refill_per_sec) * 1000.0).ceil() as i64
        } else {
            -1
        };

        Acquisition {
            allowed,
            remaining: available.floor() as i64,
            retry_after_ms,
        }
    }
}

/// Verteilter Rate-Limiter mit Fail-closed-Fallback.
pub struct RateLimiter<B: QuotaBackend> {
    primary: B,
    fallback: PodLocalBucket,
    policy: QuotaPolicy,
}

impl<B: QuotaBackend> RateLimiter<B> {
    /// Erzeugt einen Limiter mit gegebenem Backend und Standard-Politik.
    pub fn new(primary: B) -> Self {
        Self {
            primary,
            fallback: PodLocalBucket::default(),
            policy: QuotaPolicy::default(),
        }
    }

    /// Erzeugt einen Limiter mit eigener Politik.
    pub fn with_policy(primary: B, policy: QuotaPolicy) -> Self {
        Self {
            primary,
            fallback: PodLocalBucket::default(),
            policy,
        }
    }

    /// Prüft einen Tool-Aufruf gegen das Quota des Claims. Der Schlüssel wird
    /// allein aus dem Claim gebildet, nicht aus dem Tool-Namen oder -Argumenten.
    pub async fn check(&self, claims: &VerifiedClaims, now_ms: u64) -> Decision {
        let key = quota_key(claims);
        let params = self.policy.params_for(claims.role());

        match self.primary.try_acquire(&key, params, 1, now_ms).await {
            Ok(a) => Decision {
                allowed: a.allowed,
                remaining: a.remaining,
                retry_after_ms: a.retry_after_ms,
                degraded: false,
            },
            Err(_) => {
                // Fail-closed: konservatives pod-lokales Limit statt Freischaltung.
                let a = self
                    .fallback
                    .try_acquire(&key, self.policy.fallback_params(), 1, now_ms);
                Decision {
                    allowed: a.allowed,
                    remaining: a.remaining,
                    retry_after_ms: a.retry_after_ms,
                    degraded: true,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthResolver, ClaimRecord, StaticAuthResolver};
    use std::sync::Mutex as StdMutex;

    fn claims(role: Role) -> VerifiedClaims {
        StaticAuthResolver::new()
            .with_credential(
                "c",
                ClaimRecord {
                    tenant: "kanzlei-a".into(),
                    session: "sess-1".into(),
                    role,
                },
            )
            .verify("c")
            .unwrap()
    }

    /// Backend, das jeden Schlüssel mitschneidet und stets erlaubt.
    #[derive(Default)]
    struct RecordingBackend {
        keys: StdMutex<Vec<String>>,
    }

    impl QuotaBackend for RecordingBackend {
        async fn try_acquire(
            &self,
            key: &str,
            params: BucketParams,
            _cost: u32,
            _now_ms: u64,
        ) -> Result<Acquisition, QuotaError> {
            self.keys.lock().unwrap().push(key.to_string());
            Ok(Acquisition {
                allowed: true,
                remaining: params.capacity as i64,
                retry_after_ms: 0,
            })
        }
    }

    /// Backend, das immer ausfällt (simuliert Redis-Ausfall).
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

    #[tokio::test]
    async fn key_is_derived_from_claim_only() {
        let backend = RecordingBackend::default();
        let limiter = RateLimiter::new(backend);
        let c = claims(Role::Reader);

        let _ = limiter.check(&c, 1_000).await;
        let _ = limiter.check(&c, 1_001).await;

        let keys = limiter.primary.keys.lock().unwrap();
        // Beide Aufrufe treffen denselben, allein aus dem Claim gebildeten Bucket.
        assert_eq!(keys.len(), 2);
        assert!(keys.iter().all(|k| k == "quota:kanzlei-a:sess-1"));
    }

    #[tokio::test]
    async fn role_determines_bucket_capacity() {
        let policy = QuotaPolicy::default();
        assert_eq!(policy.params_for(Role::Reader).capacity, 60);
        assert_eq!(policy.params_for(Role::Navigator).capacity, 120);
        assert_eq!(policy.params_for(Role::Validator).capacity, 240);
    }

    #[tokio::test]
    async fn redis_outage_fails_closed_to_conservative_limit() {
        let limiter = RateLimiter::new(FailingBackend);
        let c = claims(Role::Validator); // hohe reguläre Kapazität (240)

        // Trotz Validator-Rolle greift im Ausfall nur das Fallback (Kapazität 5).
        let now = 10_000;
        let mut granted = 0;
        for _ in 0..20 {
            let d = limiter.check(&c, now).await;
            assert!(d.degraded, "Ausfall muss als degradiert markiert sein");
            if d.allowed {
                granted += 1;
            }
        }
        assert_eq!(
            granted, 5,
            "Fail-closed begrenzt hart auf das Fallback-Limit"
        );
    }

    #[tokio::test]
    async fn fallback_denies_when_exhausted_with_retry_hint() {
        let limiter = RateLimiter::new(FailingBackend);
        let c = claims(Role::Reader);
        let now = 50_000;
        for _ in 0..5 {
            assert!(limiter.check(&c, now).await.allowed);
        }
        let denied = limiter.check(&c, now).await;
        assert!(!denied.allowed);
        assert!(denied.degraded);
        assert!(denied.retry_after_ms > 0);
    }
}
