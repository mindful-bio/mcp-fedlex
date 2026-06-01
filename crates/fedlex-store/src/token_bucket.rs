//! Verteiltes, atomares Token-Bucket über Redis (ADR-002, Feature `redis-store`).
//!
//! Das Quota darf NICHT pod-lokal liegen. Sonst skaliert das Limit mit der
//! Pod-Zahl hoch und ein Agent umgeht es per Load-Balancing über mehrere Pods.
//! Deshalb lebt der gesamte Bucket-State in Redis und wird in EINEM Lua-Script
//! atomar gelesen, nachgefüllt, geprüft und zurückgeschrieben. Kein
//! Read-Modify-Write-Race zwischen Pods.

use redis::Script;

/// Parameter eines Token-Buckets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BucketParams {
    /// Maximale Tokenzahl (Burst-Grenze).
    pub capacity: u32,
    /// Nachfüllrate in Tokens pro Sekunde (darf gebrochen sein).
    pub refill_per_sec: f64,
    /// Lebensdauer des Bucket-Keys in Millisekunden (Aufräumen inaktiver Buckets).
    pub ttl_ms: u64,
}

/// Ergebnis eines Acquire-Versuchs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Acquisition {
    /// Ob die angeforderten Tokens gewährt wurden.
    pub allowed: bool,
    /// Verbleibende Tokens nach dem Versuch (abgerundet).
    pub remaining: i64,
    /// Wartezeit in Millisekunden bis genug Tokens vorhanden wären
    /// (`-1`, falls ohne Nachfüllung nie erreichbar).
    pub retry_after_ms: i64,
}

/// Atomares Token-Bucket über Redis.
#[derive(Clone)]
pub struct RedisTokenBucket {
    client: redis::Client,
    script: Script,
}

/// Atomare Token-Bucket-Logik. Refill nach verstrichener Zeit, dann Prüfung
/// und Abbuchung, alles serverseitig in Redis ohne Race zwischen Pods.
const TOKEN_BUCKET_LUA: &str = r#"
local capacity = tonumber(ARGV[1])
local refill   = tonumber(ARGV[2])
local now      = tonumber(ARGV[3])
local cost     = tonumber(ARGV[4])
local ttl      = tonumber(ARGV[5])

local data   = redis.call('HMGET', KEYS[1], 'tokens', 'ts')
local tokens = tonumber(data[1])
local ts     = tonumber(data[2])
if tokens == nil then
  tokens = capacity
  ts = now
end

local elapsed = now - ts
if elapsed < 0 then elapsed = 0 end
tokens = math.min(capacity, tokens + (elapsed / 1000.0) * refill)

local allowed = 0
if tokens >= cost then
  tokens = tokens - cost
  allowed = 1
end

redis.call('HMSET', KEYS[1], 'tokens', tokens, 'ts', now)
redis.call('PEXPIRE', KEYS[1], ttl)

local retry = 0
if allowed == 0 then
  if refill > 0 then
    retry = math.ceil(((cost - tokens) / refill) * 1000.0)
  else
    retry = -1
  end
end

return {allowed, math.floor(tokens), retry}
"#;

impl RedisTokenBucket {
    /// Verbindet sich mit einer Redis-URL und lädt das Lua-Script.
    pub fn connect(url: &str) -> Result<Self, super::RedisError> {
        Ok(Self {
            client: redis::Client::open(url)?,
            script: Script::new(TOKEN_BUCKET_LUA),
        })
    }

    /// Versucht, `cost` Tokens aus dem Bucket `key` abzubuchen.
    ///
    /// `now_ms` ist die aktuelle Wall-Clock in Millisekunden. Der Aufruf ist
    /// atomar. Bei `allowed == false` ist `retry_after_ms` die geschätzte
    /// Wartezeit.
    pub async fn try_acquire(
        &self,
        key: &str,
        params: BucketParams,
        cost: u32,
        now_ms: u64,
    ) -> Result<Acquisition, super::RedisError> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let (allowed, remaining, retry_after_ms): (i64, i64, i64) = self
            .script
            .key(key)
            .arg(params.capacity)
            .arg(params.refill_per_sec)
            .arg(now_ms)
            .arg(cost)
            .arg(params.ttl_ms)
            .invoke_async(&mut conn)
            .await?;

        Ok(Acquisition {
            allowed: allowed == 1,
            remaining,
            retry_after_ms,
        })
    }
}
