//! Primitive: Erlass-Suche nach Titel/Stichwort (Rulebook J3).

use crate::client::{Language, PREFIXES, SparqlClient, val};
use crate::{FEDLEX_BASE, error::JoluxError};
use serde::{Deserialize, Serialize};

/// Ein Such-Treffer: ein Erlass, der zum Suchbegriff passt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LawHit {
    /// ELI des Erlasses (relativ, `eli/cc/...`).
    pub eli: String,
    /// SR-Nummer, sofern vorhanden.
    pub sr_number: Option<String>,
    /// Titel des Treffers.
    pub title: String,
}

const SEARCH_Q: &str = r#"SELECT DISTINCT ?ca ?sr ?title WHERE {
  ?ca a jolux:ConsolidationAbstract ;
      jolux:historicalLegalId ?sr ;
      jolux:isRealizedBy ?expr .
  ?expr jolux:language <__LANGURI__> ;
        jolux:title ?title .
  FILTER(CONTAINS(LCASE(STR(?title)), LCASE("__QUERY__")))
} LIMIT __LIMIT__"#;

/// Sucht Erlasse, deren Titel den Suchbegriff enthält (case-insensitive).
///
/// **Live-verifiziert (2026-06-10):** Der amtliche Titel liegt auf der
/// Expression **direkt am CA** (`<CA> jolux:isRealizedBy ?expr`). Die
/// Expressions der Consolidations tragen nur technische Labels
/// (`"Consolidation: 730.0 - 2018-01-01"`) und sind für die Suche unbrauchbar.
///
/// **Discovery-Funktion ohne Provenance** — liefert Kandidaten, auf denen dann
/// provenance-tragende Primitive (`get_law_metadata`, `get_article_text`)
/// aufsetzen. Der Suchbegriff wird vor der Einbettung entschärft (Anführungs-
/// zeichen/Backslash entfernt), damit er die Query nicht zerbricht (kein
/// SPARQL-Injection).
pub async fn search_law(
    client: &impl SparqlClient,
    query: &str,
    lang: Language,
    limit: u32,
) -> Result<Vec<LawHit>, JoluxError> {
    let safe = query.replace(['"', '\\'], " ");
    let sparql = format!(
        "{PREFIXES}{}",
        SEARCH_Q
            .replace("__LANGURI__", lang.vocab_uri())
            .replace("__QUERY__", &safe)
            .replace("__LIMIT__", &limit.to_string())
    );
    let res = client.query(&sparql).await?;
    let hits = res
        .bindings()
        .iter()
        .filter_map(|b| {
            let ca = val(b, "ca")?;
            let title = val(b, "title")?.to_string();
            Some(LawHit {
                eli: ca.strip_prefix(FEDLEX_BASE).unwrap_or(ca).to_string(),
                sr_number: val(b, "sr").map(str::to_string),
                title,
            })
        })
        .collect();
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::MockSparqlClient;

    const FIXTURE: &str = r#"{
      "head": {"vars": ["ca","sr","title"]},
      "results": {"bindings": [
        {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2017/762"},
         "sr":{"type":"literal","value":"730.0"},
         "title":{"type":"literal","xml:lang":"de","value":"Energiegesetz (EnG)"}},
        {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/1999/404"},
         "sr":{"type":"literal","value":"101"},
         "title":{"type":"literal","xml:lang":"de","value":"Bundesverfassung"}}
      ]}
    }"#;

    #[tokio::test]
    async fn returns_hits_with_relative_eli() {
        let client = MockSparqlClient::from_json(FIXTURE);
        let hits = search_law(&client, "energie", Language::De, 10)
            .await
            .unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].eli, "eli/cc/2017/762"); // Host abgeschnitten
        assert_eq!(hits[0].sr_number.as_deref(), Some("730.0"));

        let q = client.last_query().unwrap();
        assert!(q.contains("ConsolidationAbstract"));
        assert!(q.contains("LIMIT 10"));
        assert!(q.contains(r#"LCASE("energie")"#));
    }

    #[tokio::test]
    async fn neutralizes_injection_in_query() {
        let client = MockSparqlClient::from_json(FIXTURE);
        let _ = search_law(&client, r#"a") } INJECT {"#, Language::De, 5)
            .await
            .unwrap();
        let q = client.last_query().unwrap();
        // Die Breakout-Sequenz (Quote+schliessende Klammer) darf nicht roh vorkommen ...
        assert!(!q.contains("\") }"), "Breakout-Sequenz nicht neutralisiert");
        // ... der Text bleibt aber als harmloses Literal im CONTAINS erhalten.
        assert!(q.contains("INJECT"));
    }
}
