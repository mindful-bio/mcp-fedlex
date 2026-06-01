//! Integrationstest für das verteilte Token-Bucket gegen echtes Redis (ADR-002).
//!
//! Beweist die zentrale Eigenschaft. Mehrere simulierte Pods teilen sich EIN
//! globales Limit. Feuern N Pods gleichzeitig mehr Anfragen als die Kapazität,
//! wird trotzdem nur die Kapazität gewährt, nicht N-mal die Kapazität.

#![cfg(feature = "redis-store")]

use fedlex_store::token_bucket::{BucketParams, RedisTokenBucket};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use testcontainers_modules::redis::Redis;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::test]
async fn global_limit_holds_across_simulated_pods() {
    let container = Redis::default().start().await.unwrap();
    let host = container.get_host().await.unwrap();
    let port = container.get_host_port_ipv4(6379).await.unwrap();
    let url = format!("redis://{host}:{port}");

    // Kein Refill während des Tests, damit die Gesamtzahl deterministisch ist.
    let params = BucketParams {
        capacity: 100,
        refill_per_sec: 0.0,
        ttl_ms: 60_000,
    };
    let key = "quota:kanzlei-a:sess-1";
    let now_ms: u64 = 1_000_000;

    // Vier Pods (eigene Clients, dieselbe Redis-Instanz, derselbe Bucket-Key).
    let pods = 4;
    let attempts_per_pod = 50; // 4 * 50 = 200 Versuche gegen Kapazität 100.
    let granted = Arc::new(AtomicU32::new(0));

    let mut handles = Vec::new();
    for _ in 0..pods {
        let bucket = RedisTokenBucket::connect(&url).unwrap();
        let granted = Arc::clone(&granted);
        handles.push(tokio::spawn(async move {
            for _ in 0..attempts_per_pod {
                let res = bucket.try_acquire(key, params, 1, now_ms).await.unwrap();
                if res.allowed {
                    granted.fetch_add(1, Ordering::SeqCst);
                }
            }
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    // Exakt die Kapazität wurde gewährt, nicht pods * Kapazität.
    assert_eq!(granted.load(Ordering::SeqCst), 100);
}

#[tokio::test]
async fn refill_replenishes_over_time() {
    let container = Redis::default().start().await.unwrap();
    let host = container.get_host().await.unwrap();
    let port = container.get_host_port_ipv4(6379).await.unwrap();
    let url = format!("redis://{host}:{port}");

    let bucket = RedisTokenBucket::connect(&url).unwrap();
    let params = BucketParams {
        capacity: 10,
        refill_per_sec: 10.0, // 10 Tokens pro Sekunde.
        ttl_ms: 60_000,
    };
    let key = "quota:kanzlei-b:sess-2";

    // Bucket leeren.
    let t0: u64 = 5_000_000;
    for _ in 0..10 {
        assert!(
            bucket
                .try_acquire(key, params, 1, t0)
                .await
                .unwrap()
                .allowed
        );
    }
    // Sofort erneut: erschöpft.
    let denied = bucket.try_acquire(key, params, 1, t0).await.unwrap();
    assert!(!denied.allowed);
    assert!(denied.retry_after_ms > 0);

    // 500 ms später sind ~5 Tokens nachgefüllt.
    let t1 = t0 + 500;
    let mut refilled = 0;
    for _ in 0..10 {
        if bucket
            .try_acquire(key, params, 1, t1)
            .await
            .unwrap()
            .allowed
        {
            refilled += 1;
        }
    }
    assert_eq!(refilled, 5);
}
