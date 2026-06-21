//! XML/AKN-Engine. Read-Path-Heavy-Lifting mit lokalem L1-Cache, Makro-Tools
//! und Paginierungs-Guard.
//!
//! Drei Bausteine.
//! 1. [`L1Cache`]. Hält fertig geparste DOM-Bäume pro Pod. Single-Flight via
//!    moka `get_with` garantiert genau EINEN Parse pro Schlüssel, auch unter
//!    nebenläufigen Anfragen (kein Cache-Stampede auf 50-MB-Gesetzen).
//! 2. [`diff_to_markdown`]. Verdichtet einen teuren Versionsvergleich zu
//!    destilliertem Markdown. Das LLM bekommt nie Roh-XML.
//! 3. [`paginate`]. Schützt das Context-Window vor Overflow. Rückgaben sind
//!    limitiert und tragen `{ has_more, next_page }`.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Ein Artikel-Knoten eines AKN/Jolux-Dokuments (vereinfachtes DOM).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Article {
    /// Stabile Artikel-Kennung (z.B. "art_1").
    pub id: String,
    /// Destillierter Text des Artikels.
    pub text: String,
}

/// Ein geparstes Dokument. Artikel-granular, nicht das ganze Gesetz als Blob.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Document {
    /// Artikel in Reihenfolge des Dokuments.
    pub articles: Vec<Article>,
}

impl Document {
    /// Minimaler, deterministischer Parser des internen AKN-Zeilenformats
    /// `art_id|text`. Steht stellvertretend für das native quick-xml-Parsing,
    /// ohne dessen Gewicht in die Tests zu ziehen.
    pub fn parse(raw: &str) -> Self {
        let articles = raw
            .lines()
            .filter_map(|line| line.split_once('|'))
            .map(|(id, text)| Article {
                id: id.trim().to_string(),
                text: text.trim().to_string(),
            })
            .collect();
        Self { articles }
    }

    /// Artikel als Map für effiziente Diffs.
    fn by_id(&self) -> BTreeMap<&str, &str> {
        self.articles
            .iter()
            .map(|a| (a.id.as_str(), a.text.as_str()))
            .collect()
    }
}

/// Lokaler L1-DOM-Cache pro Reader-Pod mit Single-Flight.
#[derive(Clone)]
pub struct L1Cache {
    cache: moka::future::Cache<String, Arc<Document>>,
    parse_count: Arc<AtomicUsize>,
}

impl L1Cache {
    /// Erzeugt einen Cache mit gegebener Kapazität (Anzahl Einträge).
    pub fn new(capacity: u64) -> Self {
        Self {
            cache: moka::future::Cache::new(capacity),
            parse_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Wie oft real geparst wurde. Nur für Tests/Diagnose.
    pub fn parse_count(&self) -> usize {
        self.parse_count.load(Ordering::SeqCst)
    }

    /// Liefert das geparste Dokument zum Schlüssel, parst bei Miss genau einmal.
    ///
    /// Der Schlüssel ist typischerweise `ELI + valid_as_of`. Mehrere
    /// nebenläufige Aufrufe desselben Schlüssels teilen sich denselben Parse
    /// (moka `get_with`-Single-Flight).
    pub async fn get_or_parse(&self, key: &str, raw: &str) -> Arc<Document> {
        let parse_count = Arc::clone(&self.parse_count);
        let raw = raw.to_string();
        self.cache
            .get_with(key.to_string(), async move {
                parse_count.fetch_add(1, Ordering::SeqCst);
                Arc::new(Document::parse(&raw))
            })
            .await
    }

    /// Entfernt einen Eintrag (z.B. nach einem Invalidierungs-Event des Brokers).
    pub async fn invalidate(&self, key: &str) {
        self.cache.invalidate(key).await;
    }
}

/// Verdichtet einen Versionsvergleich zweier Dokumente zu Markdown.
///
/// Liefert nur das Delta. Hinzugefügte, entfernte und geänderte Artikel. So
/// erhält das LLM eine kompakte, lesbare Zusammenfassung statt zweier XML-Bäume.
pub fn diff_to_markdown(old: &Document, new: &Document) -> String {
    let old_map = old.by_id();
    let new_map = new.by_id();

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();

    for (id, new_text) in &new_map {
        match old_map.get(id) {
            None => added.push(*id),
            Some(old_text) if old_text != new_text => changed.push(*id),
            Some(_) => {}
        }
    }
    for id in old_map.keys() {
        if !new_map.contains_key(id) {
            removed.push(*id);
        }
    }

    let mut out = String::from("# Versionsvergleich\n\n");
    let section = |out: &mut String, title: &str, ids: &[&str]| {
        out.push_str(&format!("## {} ({})\n", title, ids.len()));
        if ids.is_empty() {
            out.push_str("- keine\n");
        } else {
            for id in ids {
                out.push_str(&format!("- {id}\n"));
            }
        }
        out.push('\n');
    };
    section(&mut out, "Hinzugefuegt", &added);
    section(&mut out, "Geaendert", &changed);
    section(&mut out, "Entfernt", &removed);
    out
}

/// Eine paginierte Rückgabe mit Overflow-Schutz-Metadaten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    /// Die Elemente dieser Seite.
    pub items: Vec<T>,
    /// Ob weitere Seiten folgen.
    pub has_more: bool,
    /// Index der nächsten Seite, falls vorhanden.
    pub next_page: Option<usize>,
}

/// Schneidet `items` auf eine Seite zu und liefert Overflow-Metadaten.
///
/// `page_size` von 0 wird auf 1 angehoben, damit immer Fortschritt möglich ist.
pub fn paginate<T: Clone>(items: &[T], page: usize, page_size: usize) -> Page<T> {
    let page_size = page_size.max(1);
    let start = page.saturating_mul(page_size);
    let end = start.saturating_add(page_size).min(items.len());
    let slice = if start < items.len() {
        items[start..end].to_vec()
    } else {
        Vec::new()
    };
    let has_more = end < items.len();
    Page {
        items: slice,
        has_more,
        next_page: has_more.then(|| page + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn parses_articles() {
        let doc = Document::parse("art_1|Würde des Menschen\nart_2|Rechtsstaat");
        assert_eq!(doc.articles.len(), 2);
        assert_eq!(doc.articles[0].id, "art_1");
        assert_eq!(doc.articles[1].text, "Rechtsstaat");
    }

    #[tokio::test]
    async fn l1_single_flight_parses_once_under_concurrency() {
        let cache = L1Cache::new(64);
        let raw = "art_1|A\nart_2|B";
        let key = "eli/cc/1999/404@2020-01-01";

        // 16 nebenläufige Anfragen auf denselben Schlüssel.
        let mut handles = Vec::new();
        for _ in 0..16 {
            let cache = cache.clone();
            let raw = raw.to_string();
            handles.push(tokio::spawn(async move {
                let doc = cache.get_or_parse(key, &raw).await;
                doc.articles.len()
            }));
        }
        for h in handles {
            assert_eq!(h.await.unwrap(), 2);
        }

        // Trotz 16 Anfragen genau ein Parse (Single-Flight, kein Stampede).
        assert_eq!(cache.parse_count(), 1);
    }

    #[tokio::test]
    async fn l1_invalidation_forces_reparse() {
        let cache = L1Cache::new(64);
        let key = "k";
        let _ = cache.get_or_parse(key, "art_1|A").await;
        cache.invalidate(key).await;
        // moka invalidate ist eventually-consistent. Kurz nachgeben.
        tokio::time::sleep(Duration::from_millis(20)).await;
        let doc = cache.get_or_parse(key, "art_1|A neu").await;
        assert_eq!(doc.articles[0].text, "A neu");
        assert_eq!(cache.parse_count(), 2);
    }

    #[test]
    fn diff_distills_added_changed_removed() {
        let old = Document::parse("art_1|alt\nart_2|bleibt");
        let new = Document::parse("art_2|bleibt\nart_3|neu");
        let md = diff_to_markdown(&old, &new);
        assert!(md.contains("## Hinzugefuegt (1)"));
        assert!(md.contains("- art_3"));
        assert!(md.contains("## Entfernt (1)"));
        assert!(md.contains("- art_1"));
        assert!(md.contains("## Geaendert (0)"));
    }

    #[test]
    fn paginate_reports_has_more_and_next_page() {
        let items: Vec<u32> = (0..10).collect();
        let p0 = paginate(&items, 0, 4);
        assert_eq!(p0.items, vec![0, 1, 2, 3]);
        assert!(p0.has_more);
        assert_eq!(p0.next_page, Some(1));

        let p2 = paginate(&items, 2, 4);
        assert_eq!(p2.items, vec![8, 9]);
        assert!(!p2.has_more);
        assert_eq!(p2.next_page, None);
    }

    #[test]
    fn paginate_past_end_is_empty_not_panic() {
        let items: Vec<u32> = (0..3).collect();
        let p = paginate(&items, 99, 4);
        assert!(p.items.is_empty());
        assert!(!p.has_more);
    }
}
