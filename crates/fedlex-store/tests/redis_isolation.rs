//! Integrationstest gegen echtes Redis (Docker-gated, Feature `redis-store`).
//!
//! Beweist, dass die Tenant-Isolation aus ADR-001 nicht nur in-memory, sondern
//! auch gegen eine echte Redis-Instanz hält. Wird nur mit `--features
//! redis-store` und laufendem Docker ausgeführt; sonst ist die Datei leer.

#![cfg(feature = "redis-store")]

use fedlex_store::RedisScratchpad;
use fedlex_store::key::{SessionId, TenantContext, TenantId};
use testcontainers_modules::redis::Redis;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

fn ctx(tenant: &str, session: &str) -> TenantContext {
    TenantContext::new(
        TenantId::from_claim(tenant).unwrap(),
        SessionId::from_claim(session).unwrap(),
    )
}

#[tokio::test]
#[ignore = "braucht Docker (testcontainers); lokal mit `cargo test -- --ignored` ausführen"]
async fn cross_tenant_isolation_holds_against_real_redis() {
    // Echte Redis-Instanz im Container hochfahren.
    let container = Redis::default().start().await.unwrap();
    let host = container.get_host().await.unwrap();
    let port = container.get_host_port_ipv4(6379).await.unwrap();
    let url = format!("redis://{host}:{port}");

    let store = RedisScratchpad::connect(&url).unwrap();

    let a = ctx("kanzlei-a", "sess-1");
    let b = ctx("kanzlei-b", "sess-1");

    // A schreibt unter einem User-Key, B nutzt denselben User-Key.
    let a_key = a.key("fallnotiz").unwrap();
    store.put(&a_key, "vertraulich A").await.unwrap();

    // B sieht nichts von A (verschiedener Namespace).
    let b_key = b.key("fallnotiz").unwrap();
    assert_eq!(store.get(&b_key).await.unwrap(), None);

    // A liest sein eigenes Datum.
    assert_eq!(
        store.get(&a_key).await.unwrap().as_deref(),
        Some("vertraulich A")
    );

    // Ein Ausbruchs-Key wird bereits bei der Schlüssel-Konstruktion abgelehnt,
    // erreicht also nie Redis.
    assert!(a.key("kanzlei-b:sess-1:fallnotiz").is_err());
}

#[tokio::test]
#[ignore = "braucht Docker (testcontainers); lokal mit `cargo test -- --ignored` ausführen"]
async fn put_get_delete_roundtrip_against_real_redis() {
    let container = Redis::default().start().await.unwrap();
    let host = container.get_host().await.unwrap();
    let port = container.get_host_port_ipv4(6379).await.unwrap();
    let url = format!("redis://{host}:{port}");

    let store = RedisScratchpad::connect(&url).unwrap();
    let key = ctx("kanzlei-a", "sess-1").key("merkliste").unwrap();

    store.put(&key, "Art. 41 OR").await.unwrap();
    assert_eq!(
        store.get(&key).await.unwrap().as_deref(),
        Some("Art. 41 OR")
    );
    assert!(store.delete(&key).await.unwrap());
    assert_eq!(store.get(&key).await.unwrap(), None);
    assert!(!store.delete(&key).await.unwrap());
}
