//! Cache-Warmup mit Single-Flight gegen Stampede (Day-2, M10, Backlog B-1).
//!
//! Beim Kaltstart oder nach einer Invalidierung treffen viele Anfragen
//! gleichzeitig auf einen leeren Schlüssel. Ohne Schutz löst jede davon
//! denselben teuren Lauf gegen Oxigraph aus (Stampede). Dieser Warmup garantiert
//! genau EINEN Lauf pro Schlüssel, auch unter Nebenläufigkeit. Alle Warter
//! erhalten dasselbe Ergebnis.
//!
//! Der Lader ist fehlbar. Schlägt er fehl, wird kein Wert gespeichert und der
//! nächste Aufruf versucht es erneut. So bleibt ein vorübergehender
//! Backend-Ausfall folgenlos für den Cache.
//!
//! [`WarmupCache::warm`] zieht einen Satz Schlüssel proaktiv vor, etwa während
//! der Startup-Phase, bevor `readyz` grün meldet. So trifft der erste echte
//! Verkehr auf einen warmen Cache.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Single-Flight-Cache mit fehlbarem Lader und Lade-Zähler.
///
/// Der Wert wird als `Arc<V>` gehalten, damit Warter ihn teilen, ohne zu
/// kopieren. Der Zähler macht die Single-Flight-Eigenschaft testbar.
#[derive(Clone)]
pub struct WarmupCache<V> {
    cache: moka::future::Cache<String, Arc<V>>,
    load_count: Arc<AtomicUsize>,
}

impl<V> WarmupCache<V>
where
    V: Send + Sync + 'static,
{
    /// Erzeugt einen Cache mit gegebener Kapazität (Anzahl Einträge).
    pub fn new(capacity: u64) -> Self {
        Self {
            cache: moka::future::Cache::new(capacity),
            load_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Wie oft der Lader real lief. Nur für Tests und Diagnose.
    pub fn load_count(&self) -> usize {
        self.load_count.load(Ordering::SeqCst)
    }

    /// Liefert den Wert zum Schlüssel, lädt bei Miss genau einmal.
    ///
    /// Mehrere nebenläufige Aufrufe desselben Schlüssels teilen sich denselben
    /// Lauf (moka `try_get_with`-Single-Flight). Schlägt der Lader fehl, wird
    /// nichts gespeichert und der Fehler an alle Warter gereicht.
    pub async fn get_or_load<F, Fut, E>(&self, key: &str, loader: F) -> Result<Arc<V>, Arc<E>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<V, E>>,
        E: Send + Sync + 'static,
    {
        let load_count = Arc::clone(&self.load_count);
        self.cache
            .try_get_with(key.to_string(), async move {
                let value = loader().await?;
                load_count.fetch_add(1, Ordering::SeqCst);
                Ok(Arc::new(value))
            })
            .await
    }

    /// Zieht einen Satz Schlüssel proaktiv vor.
    ///
    /// Für jeden noch nicht vorhandenen Schlüssel läuft der Lader genau einmal.
    /// Bereits warme Schlüssel werden übersprungen. Liefert die Zahl der
    /// erfolgreich vorgewärmten Schlüssel. Fehlschläge werden gezählt zurück,
    /// ohne den restlichen Lauf abzubrechen.
    pub async fn warm<K, F, Fut, E>(&self, keys: K, mut loader: F) -> WarmupReport
    where
        K: IntoIterator<Item = String>,
        F: FnMut(String) -> Fut,
        Fut: Future<Output = Result<V, E>>,
        E: Send + Sync + 'static,
    {
        let mut report = WarmupReport::default();
        for key in keys {
            if self.cache.contains_key(&key) {
                report.skipped += 1;
                continue;
            }
            let fut = loader(key.clone());
            match self.get_or_load(&key, || fut).await {
                Ok(_) => report.warmed += 1,
                Err(_) => report.failed += 1,
            }
        }
        report
    }

    /// Entfernt einen Eintrag, etwa nach einem Invalidierungs-Event des Brokers.
    /// Der nächste Aufruf lädt wieder genau einmal.
    pub async fn invalidate(&self, key: &str) {
        self.cache.invalidate(key).await;
    }
}

/// Ergebnis eines proaktiven Warmup-Laufs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WarmupReport {
    /// Frisch vorgewärmte Schlüssel.
    pub warmed: usize,
    /// Bereits warme, übersprungene Schlüssel.
    pub skipped: usize,
    /// Schlüssel, deren Lader fehlschlug.
    pub failed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Fehlertyp eines fehlschlagenden Laders.
    #[derive(Debug, PartialEq, Eq)]
    struct LoadError(&'static str);

    #[tokio::test]
    async fn single_flight_loads_once_under_concurrency() {
        let cache: WarmupCache<String> = WarmupCache::new(64);
        let key = "eli/cc/1999/404@2020-01-01";

        // 16 nebenläufige Anfragen auf denselben kalten Schlüssel.
        let mut handles = Vec::new();
        for _ in 0..16 {
            let cache = cache.clone();
            handles.push(tokio::spawn(async move {
                cache
                    .get_or_load(key, || async {
                        // Teurer Lauf, hier simuliert.
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        Ok::<_, LoadError>("Würde des Menschen".to_string())
                    })
                    .await
                    .map(|v| v.as_str().to_string())
            }));
        }
        for h in handles {
            assert_eq!(h.await.unwrap().unwrap(), "Würde des Menschen");
        }

        // Trotz 16 Anfragen genau ein Lauf (Single-Flight, kein Stampede).
        assert_eq!(cache.load_count(), 1);
    }

    #[tokio::test]
    async fn invalidation_triggers_exactly_one_reload() {
        let cache: WarmupCache<String> = WarmupCache::new(64);
        let key = "k";

        let v = cache
            .get_or_load(key, || async { Ok::<_, LoadError>("alt".to_string()) })
            .await
            .unwrap();
        assert_eq!(v.as_str(), "alt");
        assert_eq!(cache.load_count(), 1);

        cache.invalidate(key).await;
        // moka invalidate ist eventually-consistent. Kurz nachgeben.
        tokio::time::sleep(Duration::from_millis(20)).await;

        let v = cache
            .get_or_load(key, || async { Ok::<_, LoadError>("neu".to_string()) })
            .await
            .unwrap();
        assert_eq!(v.as_str(), "neu");
        assert_eq!(cache.load_count(), 2);
    }

    #[tokio::test]
    async fn failed_load_is_not_cached_and_retries() {
        let cache: WarmupCache<String> = WarmupCache::new(64);
        let key = "flaky";

        let err = cache
            .get_or_load(key, || async {
                Err::<String, _>(LoadError("backend down"))
            })
            .await
            .unwrap_err();
        assert_eq!(*err, LoadError("backend down"));
        // Kein Wert gespeichert, kein erfolgreicher Lauf gezählt.
        assert_eq!(cache.load_count(), 0);

        // Der nächste Versuch lädt erneut und gelingt.
        let v = cache
            .get_or_load(key, || async {
                Ok::<_, LoadError>("recovered".to_string())
            })
            .await
            .unwrap();
        assert_eq!(v.as_str(), "recovered");
        assert_eq!(cache.load_count(), 1);
    }

    #[tokio::test]
    async fn warm_loads_cold_keys_and_skips_warm_ones() {
        let cache: WarmupCache<String> = WarmupCache::new(64);

        // Ein Schlüssel ist schon warm.
        let _ = cache
            .get_or_load("a", || async { Ok::<_, LoadError>("A".to_string()) })
            .await
            .unwrap();
        assert_eq!(cache.load_count(), 1);

        let report = cache
            .warm(
                ["a".to_string(), "b".to_string(), "c".to_string()],
                |key| async move { Ok::<_, LoadError>(key.to_uppercase()) },
            )
            .await;

        assert_eq!(report.skipped, 1);
        assert_eq!(report.warmed, 2);
        assert_eq!(report.failed, 0);
        // Insgesamt drei Läufe (a vorab, b und c im Warmup).
        assert_eq!(cache.load_count(), 3);
    }

    #[tokio::test]
    async fn warm_counts_failures_without_aborting() {
        let cache: WarmupCache<String> = WarmupCache::new(64);

        let report = cache
            .warm(
                ["ok".to_string(), "bad".to_string(), "ok2".to_string()],
                |key| async move {
                    if key == "bad" {
                        Err(LoadError("nope"))
                    } else {
                        Ok::<_, LoadError>(key)
                    }
                },
            )
            .await;

        assert_eq!(report.warmed, 2);
        assert_eq!(report.failed, 1);
        assert_eq!(report.skipped, 0);
    }
}
