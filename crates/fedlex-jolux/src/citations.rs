//! Primitive: Zitationsgraph (Lexikon JLX-CIT-01, Rulebook J7).
//!
//! JOLux-Zitationen existieren nur auf Gesamttext-Granularität (`/text`),
//! nie auf Artikel-Ebene (J7.1). Der Overlap mit AKN-Inline-`<ref>` beträgt
//! nur 0–48 % (J7.3) — vollständige Zitationsnetze brauchen den **Merge**
//! beider Quellen.

use crate::client::{val, SparqlClient, PREFIXES};
use crate::{eli_uri, error::JoluxError};
use fedlex_core::{Eli, Provenance, Response, TransactionTime, ValidAsOf};
use serde::{Deserialize, Serialize};

/// Richtung der Zitations-Abfrage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CitationDirection {
    /// Was zitiert dieser Erlass? (`citationFromLegalResource` = Erlass)
    Outgoing,
    /// Wer zitiert diesen Erlass? (`citationToLegalResource` = Erlass)
    Incoming,
    /// Beide Richtungen (UNION).
    Both,
}

/// Eine formale Zitation zwischen zwei Erlassen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    /// Quelle der Zitation.
    pub from: String,
    /// Ziel der Zitation.
    pub to: String,
    /// Beschreibung der Fundstelle (seit ~2026 von Fedlex befüllt,
    /// Rulebook J7.2 überholt — trotzdem optional behandeln).
    pub description: Option<String>,
}

const CIT_OUT: &str = r#"  ?cit jolux:citationFromLegalResource ?from ;
       jolux:citationToLegalResource ?to .
  OPTIONAL { ?cit jolux:descriptionFrom ?desc }
  FILTER(STRSTARTS(STR(?from), "__URI__"))"#;

const CIT_IN: &str = r#"  ?cit jolux:citationFromLegalResource ?from ;
       jolux:citationToLegalResource ?to .
  OPTIONAL { ?cit jolux:descriptionFrom ?desc }
  FILTER(STRSTARTS(STR(?to), "__URI__"))"#;

/// JLX-CIT-01: Formale Zitationen eines Erlasses (ein- und/oder ausgehend).
///
/// Dedupliziert nach `(from, to)` — der Graph führt Zitationen pro Fassung
/// mehrfach (J7.4). **Nicht** mit Vollständigkeit verwechseln: für das echte
/// Zitationsnetz JOLux ⊕ AKN-Refs mergen (J7.3).
pub async fn get_citations(
    client: &impl SparqlClient,
    eli: &Eli,
    direction: CitationDirection,
    as_of: ValidAsOf,
) -> Result<Response<Vec<Citation>>, JoluxError> {
    let uri = eli_uri(eli);
    // Live-Befund (2026-06-10). Die Fedlex-WAF blockt `SELECT DISTINCT` in
    // Kombination mit `citationFromLegalResource` und einem URL-Literal
    // (HTTP 400, SQL-Injection-Heuristik). Daher hier ohne DISTINCT —
    // die Deduplikation nach (from, to) geschieht ohnehin clientseitig.
    let body = match direction {
        CitationDirection::Outgoing => format!("SELECT ?from ?to ?desc WHERE {{\n{CIT_OUT}\n}}"),
        CitationDirection::Incoming => format!("SELECT ?from ?to ?desc WHERE {{\n{CIT_IN}\n}}"),
        CitationDirection::Both => format!(
            "SELECT ?from ?to ?desc WHERE {{\n  {{\n{CIT_OUT}\n  }} UNION {{\n{CIT_IN}\n  }}\n}}"
        ),
    };
    let sparql = format!("{PREFIXES}{}", body.replace("__URI__", &uri));
    let res = client.query(&sparql).await?;

    let mut citations: Vec<Citation> = Vec::new();
    for b in res.bindings() {
        let (Some(from), Some(to)) = (val(b, "from"), val(b, "to")) else {
            continue;
        };
        // Dedup nach (from, to) — Quad-Store-Duplikate pro Fassung (J7.4).
        if citations.iter().any(|c| c.from == from && c.to == to) {
            continue;
        }
        citations.push(Citation {
            from: from.to_string(),
            to: to.to_string(),
            description: val(b, "desc").map(str::to_string),
        });
    }

    let prov = Provenance::new(eli.clone(), as_of, TransactionTime::now());
    Ok(Response::new(citations, prov))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::MockSparqlClient;
    use time::macros::date;

    const FIXTURE: &str = r#"{
      "head": {"vars": ["from","to","desc"]},
      "results": {"bindings": [
        {"from":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2017/762/text"},
         "to":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/1998/3033/text"},
         "desc":{"type":"literal","value":"Art. 31"}},
        {"from":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2017/762/text"},
         "to":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/1998/3033/text"}},
        {"from":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2017/762/text"},
         "to":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/1999/404/text"}}
      ]}
    }"#;

    #[tokio::test]
    async fn deduplicates_citations_per_target() {
        let client = MockSparqlClient::from_json(FIXTURE);
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = get_citations(
            &client,
            &eli,
            CitationDirection::Outgoing,
            ValidAsOf::new(date!(2026 - 01 - 01)),
        )
        .await
        .unwrap();
        assert_eq!(resp.data().len(), 2, "Duplikat (J7.4) nicht entfernt");
        assert_eq!(resp.data()[0].description.as_deref(), Some("Art. 31"));

        let q = client.last_query().unwrap();
        assert!(
            q.contains(r#"STRSTARTS(STR(?from), "https://fedlex.data.admin.ch/eli/cc/2017/762")"#)
        );
        assert!(!q.contains("UNION"));
    }

    #[tokio::test]
    async fn both_direction_uses_union() {
        let client = MockSparqlClient::from_json(FIXTURE);
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let _ = get_citations(
            &client,
            &eli,
            CitationDirection::Both,
            ValidAsOf::new(date!(2026 - 01 - 01)),
        )
        .await
        .unwrap();
        let q = client.last_query().unwrap();
        assert!(q.contains("UNION"));
        assert!(q.contains("STRSTARTS(STR(?to),"));
    }
}
