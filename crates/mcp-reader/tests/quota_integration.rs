//! End-to-End-Quota gegen echtes Redis (ADR-002, Docker-gated).
//!
//! Beweist den vollen Reader-Pfad. Der [`RateLimiter`] über das
//! [`RedisQuotaBackend`] drosselt anhand des Claim-gebundenen Buckets. Zwei
//! Sessions desselben Mandanten treffen getrennte Buckets, das Limit einer
//! Session erschöpft die andere nicht.

use mcp_reader::auth::{AuthResolver, ClaimRecord, Role, StaticAuthResolver, VerifiedClaims};
use mcp_reader::quota::{QuotaPolicy, RateLimiter, RedisQuotaBackend};
use testcontainers_modules::redis::Redis;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

fn claims(session: &str, role: Role) -> VerifiedClaims {
    StaticAuthResolver::new()
        .with_credential(
            "c",
            ClaimRecord {
                tenant: "kanzlei-a".into(),
                session: session.into(),
                role,
            },
        )
        .verify("c")
        .unwrap()
}

#[tokio::test]
#[ignore = "braucht Docker (testcontainers); lokal mit `cargo test -- --ignored` ausführen"]
async fn reader_enforces_distributed_quota_per_claim() {
    let container = Redis::default().start().await.unwrap();
    let host = container.get_host().await.unwrap();
    let port = container.get_host_port_ipv4(6379).await.unwrap();
    let url = format!("redis://{host}:{port}");

    // Reader-Rolle. Kapazität 60, hier ohne nennenswerten Refill durch festen
    // Zeitstempel (now_ms konstant).
    let backend = RedisQuotaBackend::connect(&url).unwrap();
    let limiter = RateLimiter::with_policy(backend, QuotaPolicy::default());

    let a = claims("sess-1", Role::Reader);
    let now = 1_000_000;

    // Kapazität (60) wird gewährt, danach gedrosselt.
    let mut granted = 0;
    for _ in 0..80 {
        let d = limiter.check(&a, now).await;
        assert!(!d.degraded, "Redis ist verfügbar, kein Fallback erwartet");
        if d.allowed {
            granted += 1;
        }
    }
    assert_eq!(granted, 60);

    // Eine zweite Session desselben Mandanten hat ihren eigenen Bucket.
    let b = claims("sess-2", Role::Reader);
    let d = limiter.check(&b, now).await;
    assert!(d.allowed, "fremde Session darf nicht miterschöpft sein");
}
