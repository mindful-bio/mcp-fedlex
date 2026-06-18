//! Redis-Implementierung des Scratchpads (Feature `redis-store`).
//!
//! Produktions-L2-Speicher. Async (tokio), weil der Reader async ist. Die
//! Tenant-Isolation liegt im [`ScratchpadKey`] und ist damit identisch zur
//! In-Memory-Variante. Diese Implementierung beweist im Docker-gated
//! Integrationstest, dass die Invariante auch gegen echtes Redis hält.

use crate::key::ScratchpadKey;
use redis::AsyncCommands;

/// Fehler der Redis-Anbindung.
#[derive(Debug, thiserror::Error)]
pub enum RedisError {
    /// Fehler aus der darunterliegenden Redis-Bibliothek.
    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),
    /// Fehler beim Aufbau der gegenseitig authentifizierten TLS-Verbindung
    /// (ADR-005): fehlendes/ungültiges Zertifikatsmaterial oder ein
    /// Klartext-Schema, wo mTLS verlangt ist.
    #[error("redis tls error: {0}")]
    Tls(String),
}

/// Async-Scratchpad über Redis.
#[derive(Clone)]
pub struct RedisScratchpad {
    client: redis::Client,
}

impl RedisScratchpad {
    /// Verbindet sich mit einer Redis-URL (z.B. `redis://127.0.0.1:6379`).
    pub fn connect(url: &str) -> Result<Self, RedisError> {
        Ok(Self {
            client: redis::Client::open(url)?,
        })
    }

    /// Legt einen Wert unter dem genamespacten Schlüssel ab.
    pub async fn put(&self, key: &ScratchpadKey, value: &str) -> Result<(), RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let _: () = conn.set(key.as_str(), value).await?;
        Ok(())
    }

    /// Liest einen Wert, falls vorhanden.
    pub async fn get(&self, key: &ScratchpadKey) -> Result<Option<String>, RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let value: Option<String> = conn.get(key.as_str()).await?;
        Ok(value)
    }

    /// Entfernt einen Wert und meldet, ob etwas entfernt wurde.
    pub async fn delete(&self, key: &ScratchpadKey) -> Result<bool, RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let removed: i64 = conn.del(key.as_str()).await?;
        Ok(removed > 0)
    }
}
