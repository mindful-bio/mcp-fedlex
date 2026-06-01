//! Circuit Breaker gegen kaskadierende Ausfälle externer LOD-Endpunkte.
//!
//! Ein langsamer oder defekter externer SPARQL-Endpunkt darf den Reader nicht
//! mitreissen. Ohne Breaker stauen sich Tasks an offenen Sockets, bis der Pod
//! kippt. Der Breaker kennt drei Zustände (ADR-nah).
//! - `Closed`. Normalbetrieb. Aufrufe gehen durch, Fehler werden gezählt.
//! - `Open`. Nach `failure_threshold` Fehlern in Folge. Aufrufe scheitern sofort
//!   (Fail-Fast), kein neuer externer Call, kein Task-Aufstauen. Nach
//!   `open_cooldown` wechselt der Breaker selbsttätig zu `HalfOpen`.
//! - `HalfOpen`. Ein einzelner Probe-Aufruf darf durch. Erfolg schliesst den
//!   Breaker, Fehler öffnet ihn erneut.

use std::sync::Mutex;
use std::time::Duration;
use time::OffsetDateTime;

/// Zustand des Breakers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    /// Normalbetrieb, Aufrufe gehen durch.
    Closed,
    /// Gesperrt, Aufrufe scheitern sofort bis zum Cooldown-Ablauf.
    Open,
    /// Ein Probe-Aufruf ist erlaubt.
    HalfOpen,
}

/// Fehler des Breakers gegenüber dem Aufrufer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BreakerError<E> {
    /// Der Breaker ist offen, der Aufruf wurde gar nicht erst durchgelassen.
    #[error("circuit open, call short-circuited")]
    Open,
    /// Der durchgelassene Aufruf scheiterte mit dem Upstream-Fehler.
    #[error("upstream call failed")]
    Upstream(E),
}

/// Konfiguration der Schwellen.
#[derive(Debug, Clone, Copy)]
pub struct BreakerConfig {
    /// Anzahl aufeinanderfolgender Fehler bis zum Öffnen.
    pub failure_threshold: u32,
    /// Sperrzeit, bevor ein Probe-Aufruf erlaubt wird.
    pub open_cooldown: Duration,
}

impl Default for BreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            open_cooldown: Duration::from_secs(30),
        }
    }
}

#[derive(Debug)]
struct Inner {
    state: BreakerState,
    consecutive_failures: u32,
    opened_at: Option<OffsetDateTime>,
}

/// Thread-sicherer Circuit Breaker. Teilbar über `Arc`.
#[derive(Debug)]
pub struct CircuitBreaker {
    config: BreakerConfig,
    inner: Mutex<Inner>,
}

impl CircuitBreaker {
    /// Erzeugt einen geschlossenen Breaker mit gegebener Konfiguration.
    pub fn new(config: BreakerConfig) -> Self {
        Self {
            config,
            inner: Mutex::new(Inner {
                state: BreakerState::Closed,
                consecutive_failures: 0,
                opened_at: None,
            }),
        }
    }

    /// Aktueller Zustand. Berücksichtigt einen fälligen Cooldown-Übergang.
    pub fn state(&self) -> BreakerState {
        let mut inner = self.inner.lock().expect("breaker mutex poisoned");
        self.refresh(&mut inner, OffsetDateTime::now_utc());
        inner.state
    }

    /// Übergang Open -> HalfOpen, sobald der Cooldown abgelaufen ist.
    fn refresh(&self, inner: &mut Inner, now: OffsetDateTime) {
        if inner.state == BreakerState::Open {
            if let Some(opened) = inner.opened_at {
                if now - opened >= self.config.open_cooldown {
                    inner.state = BreakerState::HalfOpen;
                }
            }
        }
    }

    /// Prüft, ob ein Aufruf durchgelassen wird, ohne ihn auszuführen.
    fn allow(&self) -> bool {
        let mut inner = self.inner.lock().expect("breaker mutex poisoned");
        self.refresh(&mut inner, OffsetDateTime::now_utc());
        inner.state != BreakerState::Open
    }

    fn on_success(&self) {
        let mut inner = self.inner.lock().expect("breaker mutex poisoned");
        inner.consecutive_failures = 0;
        inner.state = BreakerState::Closed;
        inner.opened_at = None;
    }

    fn on_failure(&self) {
        let mut inner = self.inner.lock().expect("breaker mutex poisoned");
        // Ein Fehler im HalfOpen-Zustand öffnet sofort wieder.
        if inner.state == BreakerState::HalfOpen {
            inner.state = BreakerState::Open;
            inner.opened_at = Some(OffsetDateTime::now_utc());
            return;
        }
        inner.consecutive_failures += 1;
        if inner.consecutive_failures >= self.config.failure_threshold {
            inner.state = BreakerState::Open;
            inner.opened_at = Some(OffsetDateTime::now_utc());
        }
    }

    /// Führt `call` nur aus, wenn der Breaker nicht offen ist.
    ///
    /// Bei offenem Breaker kehrt der Aufruf sofort mit [`BreakerError::Open`]
    /// zurück, ohne `call` auch nur anzustossen. Genau das verhindert das
    /// Task-Aufstauen gegen einen toten Endpunkt.
    pub async fn call<T, E, F, Fut>(&self, call: F) -> Result<T, BreakerError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
    {
        if !self.allow() {
            return Err(BreakerError::Open);
        }
        match call().await {
            Ok(value) => {
                self.on_success();
                Ok(value)
            }
            Err(err) => {
                self.on_failure();
                Err(BreakerError::Upstream(err))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[derive(Debug, PartialEq, Eq)]
    struct Boom;

    #[tokio::test]
    async fn opens_after_threshold_and_short_circuits() {
        let breaker = CircuitBreaker::new(BreakerConfig {
            failure_threshold: 3,
            open_cooldown: Duration::from_secs(30),
        });
        let calls = Arc::new(AtomicUsize::new(0));

        // Drei Fehler in Folge öffnen den Breaker.
        for _ in 0..3 {
            let calls = Arc::clone(&calls);
            let r: Result<(), BreakerError<Boom>> = breaker
                .call(|| async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Err(Boom)
                })
                .await;
            assert!(matches!(r, Err(BreakerError::Upstream(Boom))));
        }
        assert_eq!(breaker.state(), BreakerState::Open);
        assert_eq!(calls.load(Ordering::SeqCst), 3);

        // Weitere Aufrufe scheitern sofort, OHNE den Endpunkt zu berühren.
        for _ in 0..10 {
            let calls = Arc::clone(&calls);
            let r: Result<(), BreakerError<Boom>> = breaker
                .call(|| async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Err(Boom)
                })
                .await;
            assert!(matches!(r, Err(BreakerError::Open)));
        }
        // Kein einziger zusätzlicher Call gegen den toten Endpunkt.
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn half_open_probe_closes_on_success() {
        let breaker = CircuitBreaker::new(BreakerConfig {
            failure_threshold: 1,
            // Cooldown praktisch null, damit der Probe-Aufruf sofort darf.
            open_cooldown: Duration::ZERO,
        });
        let _: Result<(), BreakerError<Boom>> = breaker.call(|| async { Err(Boom) }).await;
        assert_eq!(breaker.state(), BreakerState::HalfOpen);

        // Erfolgreicher Probe-Aufruf schliesst den Breaker.
        let ok: Result<u32, BreakerError<Boom>> = breaker.call(|| async { Ok(7) }).await;
        assert_eq!(ok.unwrap(), 7);
        assert_eq!(breaker.state(), BreakerState::Closed);
    }

    #[tokio::test]
    async fn half_open_probe_reopens_on_failure() {
        let breaker = CircuitBreaker::new(BreakerConfig {
            failure_threshold: 1,
            // Kurzer, positiver Cooldown. So lässt sich nach dem Ablauf der
            // HalfOpen-Probe das erneute Öffnen beobachten, ohne dass ein
            // Null-Cooldown den Zustand sofort wieder zu HalfOpen schiebt.
            open_cooldown: Duration::from_millis(40),
        });
        let _: Result<(), BreakerError<Boom>> = breaker.call(|| async { Err(Boom) }).await;
        // Cooldown abwarten, damit der Breaker einen Probe-Aufruf erlaubt.
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(breaker.state(), BreakerState::HalfOpen);

        // Der Probe-Aufruf scheitert und öffnet sofort wieder. Direkt danach,
        // noch vor Ablauf des frischen Cooldowns, ist der Zustand Open.
        let _: Result<(), BreakerError<Boom>> = breaker.call(|| async { Err(Boom) }).await;
        assert_eq!(breaker.state(), BreakerState::Open);
    }

    #[tokio::test]
    async fn success_resets_failure_count() {
        let breaker = CircuitBreaker::new(BreakerConfig {
            failure_threshold: 3,
            open_cooldown: Duration::from_secs(30),
        });
        let _: Result<(), BreakerError<Boom>> = breaker.call(|| async { Err(Boom) }).await;
        let _: Result<u32, BreakerError<Boom>> = breaker.call(|| async { Ok(1) }).await;
        // Nach dem Erfolg ist der Zähler zurückgesetzt, zwei weitere Fehler
        // öffnen also noch nicht.
        let _: Result<(), BreakerError<Boom>> = breaker.call(|| async { Err(Boom) }).await;
        let _: Result<(), BreakerError<Boom>> = breaker.call(|| async { Err(Boom) }).await;
        assert_eq!(breaker.state(), BreakerState::Closed);
    }
}
