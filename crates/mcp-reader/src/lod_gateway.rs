//! LOD-Gateway. Föderierte Auflösung von ELI/ECLI-Referenzen.
//!
//! Eine Referenz wird zuerst lokal gesucht (eigener Oxigraph-/Store-Bestand).
//! Nur wenn sie lokal fehlt, geht ein externer Call hinaus, und zwar
//! ausschliesslich durch den [`CircuitBreaker`]. So bleibt der Reader bei einem
//! kaputten externen Endpunkt antwortfähig, lokale Referenzen kosten nie einen
//! Netz-Hop.

use std::collections::BTreeSet;

use crate::circuit_breaker::{BreakerError, CircuitBreaker};

/// Woher eine Auflösung stammt. Macht den Lokal-vs-Extern-Pfad prüfbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Aus dem lokalen Bestand, ohne externen Call.
    Local,
    /// Über einen externen Endpunkt, durch den Breaker.
    External,
}

/// Ergebnis einer Auflösung samt Herkunft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// Die aufgelöste Ziel-Referenz (z.B. kanonische ELI).
    pub target: String,
    /// Herkunft der Auflösung.
    pub origin: Origin,
}

/// Fehler der Auflösung.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    /// Weder lokal vorhanden noch extern erreichbar.
    #[error("reference could not be resolved: {0}")]
    NotFound(String),
    /// Der externe Pfad war durch den offenen Breaker gesperrt.
    #[error("external resolution unavailable (circuit open)")]
    Unavailable,
}

impl ResolveError {
    /// Lenkender Hinweis fürs LLM.
    pub fn hint(&self) -> &'static str {
        match self {
            ResolveError::NotFound(_) => "Referenz unbekannt, bitte ELI/ECLI pruefen.",
            ResolveError::Unavailable => {
                "Externe Quelle voruebergehend nicht erreichbar, lokale Navigation weiterhin moeglich."
            }
        }
    }
}

/// Externer Konnektor (SPARQL-Walk gegen einen fremden LOD-Endpunkt).
///
/// Als Trait, damit Tests einen langsamen oder defekten Endpunkt injizieren.
#[async_trait::async_trait]
pub trait ExternalConnector: Send + Sync {
    /// Löst eine Referenz extern auf. Fehler signalisiert einen Endpunkt-Ausfall.
    async fn fetch(&self, reference: &str) -> Result<String, ConnectorError>;
}

/// Fehler des externen Konnektors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("external connector failure: {0}")]
pub struct ConnectorError(pub String);

/// Auflöser über lokalen Bestand und (durch den Breaker) externen Konnektor.
pub struct EliResolver<C: ExternalConnector> {
    local: BTreeSet<String>,
    connector: C,
    breaker: CircuitBreaker,
}

impl<C: ExternalConnector> EliResolver<C> {
    /// Erzeugt den Auflöser mit lokalem Bestand, Konnektor und Breaker.
    pub fn new(
        local: impl IntoIterator<Item = String>,
        connector: C,
        breaker: CircuitBreaker,
    ) -> Self {
        Self {
            local: local.into_iter().collect(),
            connector,
            breaker,
        }
    }

    /// Zugriff auf den Breaker, etwa zur Zustandsprüfung.
    pub fn breaker(&self) -> &CircuitBreaker {
        &self.breaker
    }

    /// Löst eine Referenz auf. Lokal zuerst, extern nur durch den Breaker.
    pub async fn resolve(&self, reference: &str) -> Result<Resolved, ResolveError> {
        // Lokaler Treffer. Kein externer Call.
        if self.local.contains(reference) {
            return Ok(Resolved {
                target: reference.to_string(),
                origin: Origin::Local,
            });
        }

        // Extern, ausschliesslich durch den Breaker.
        let result = self
            .breaker
            .call(|| async { self.connector.fetch(reference).await })
            .await;

        match result {
            Ok(target) => Ok(Resolved {
                target,
                origin: Origin::External,
            }),
            Err(BreakerError::Open) => Err(ResolveError::Unavailable),
            Err(BreakerError::Upstream(_)) => Err(ResolveError::NotFound(reference.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit_breaker::{BreakerConfig, BreakerState};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// Konnektor, der Aufrufe zählt und stets scheitert (defekter Endpunkt).
    struct FailingConnector {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl ExternalConnector for FailingConnector {
        async fn fetch(&self, _reference: &str) -> Result<String, ConnectorError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(ConnectorError("timeout".into()))
        }
    }

    /// Konnektor, der jeden Call zählt und erfolgreich antwortet.
    struct OkConnector {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl ExternalConnector for OkConnector {
        async fn fetch(&self, reference: &str) -> Result<String, ConnectorError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(format!("canonical/{reference}"))
        }
    }

    #[tokio::test]
    async fn local_reference_resolves_without_external_call() {
        let calls = Arc::new(AtomicUsize::new(0));
        let resolver = EliResolver::new(
            ["eli/cc/1999/404".to_string()],
            OkConnector {
                calls: Arc::clone(&calls),
            },
            CircuitBreaker::new(BreakerConfig::default()),
        );

        let r = resolver.resolve("eli/cc/1999/404").await.unwrap();
        assert_eq!(r.origin, Origin::Local);
        // Kein externer Hop für lokale Referenzen.
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn external_reference_goes_through_connector() {
        let calls = Arc::new(AtomicUsize::new(0));
        let resolver = EliResolver::new(
            std::iter::empty::<String>(),
            OkConnector {
                calls: Arc::clone(&calls),
            },
            CircuitBreaker::new(BreakerConfig::default()),
        );
        let r = resolver.resolve("eli/eu/2016/679").await.unwrap();
        assert_eq!(r.origin, Origin::External);
        assert_eq!(r.target, "canonical/eli/eu/2016/679");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn broken_endpoint_opens_breaker_and_stops_piling_up() {
        let calls = Arc::new(AtomicUsize::new(0));
        let resolver = EliResolver::new(
            std::iter::empty::<String>(),
            FailingConnector {
                calls: Arc::clone(&calls),
            },
            CircuitBreaker::new(BreakerConfig {
                failure_threshold: 3,
                open_cooldown: Duration::from_secs(30),
            }),
        );

        // Drei Auflösungsversuche scheitern und öffnen den Breaker.
        for _ in 0..3 {
            let e = resolver.resolve("eli/eu/x").await.unwrap_err();
            assert!(matches!(e, ResolveError::NotFound(_)));
        }
        assert_eq!(resolver.breaker().state(), BreakerState::Open);
        assert_eq!(calls.load(Ordering::SeqCst), 3);

        // Weitere Versuche werden kurzgeschlossen, der defekte Endpunkt wird
        // nicht mehr berührt. Kein Task-Aufstauen.
        for _ in 0..20 {
            let e = resolver.resolve("eli/eu/x").await.unwrap_err();
            assert_eq!(e, ResolveError::Unavailable);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }
}
