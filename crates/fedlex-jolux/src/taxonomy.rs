//! Primitive: Thematische Navigation über die Rechtstaxonomie
//! (Lexikon JLX-TAX-01/02, Rulebook J20.3).
//!
//! Die deterministische Brücke für Cross-Law-Navigation — komplementär zu
//! semantic-fedlex (Embedding-basiert, probabilistisch). Deckt 85.4 % der
//! CAs ab; der Rest ist nur über Vektor-Suche erreichbar.

use crate::client::{val, Language, SparqlClient, PREFIXES};
use crate::{eli_uri, error::JoluxError, FEDLEX_BASE};
use fedlex_core::{Eli, Provenance, Response, TransactionTime, ValidAsOf};
use serde::{Deserialize, Serialize};

/// Ein Taxonomie-Eintrag, dem ein Erlass zugeordnet ist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxonomyEntry {
    /// URI des Taxonomie-Eintrags (`legal-taxonomy`).
    pub uri: String,
    /// SKOS-Label in der angefragten Sprache, sofern vorhanden.
    pub label: Option<String>,
    /// Übergeordneter Eintrag (`skos:broader`), sofern vorhanden.
    pub broader: Option<String>,
}

const TAX_Q: &str = r#"SELECT DISTINCT ?tax ?label ?parent WHERE {
  <__URI__> jolux:classifiedByTaxonomyEntry ?tax .
  OPTIONAL { ?tax skos:prefLabel ?label . FILTER(LANG(?label) = "__TAG__") }
  OPTIONAL { ?tax skos:broader ?parent }
} LIMIT 50"#;

/// JLX-TAX-01: In welchem Rechtsgebiet steht dieses Gesetz?
///
/// Liefert die Taxonomie-Einträge mit aufgelöstem Label und `skos:broader`-
/// Parent für Hierarchie-Traversal. 10'132 CAs sind nicht klassifiziert
/// (J20.3) — leere Liste ist dann normal.
pub async fn get_taxonomy(
    client: &impl SparqlClient,
    eli: &Eli,
    as_of: ValidAsOf,
    lang: Language,
) -> Result<Response<Vec<TaxonomyEntry>>, JoluxError> {
    let sparql = format!(
        "{PREFIXES}{}",
        TAX_Q
            .replace("__URI__", &eli_uri(eli))
            .replace("__TAG__", lang.tag())
    );
    let res = client.query(&sparql).await?;
    let entries = res
        .bindings()
        .iter()
        .filter_map(|b| {
            Some(TaxonomyEntry {
                uri: val(b, "tax")?.to_string(),
                label: val(b, "label").map(str::to_string),
                broader: val(b, "parent").map(str::to_string),
            })
        })
        .collect();
    let prov = Provenance::new(eli.clone(), as_of, TransactionTime::now());
    Ok(Response::new(entries, prov))
}

/// Ein thematisch verwandter Erlass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedLaw {
    /// ELI des verwandten Erlasses (relativ).
    pub eli: String,
    /// SR-Nummer, sofern vorhanden.
    pub sr_number: Option<String>,
}

const RELATED_Q: &str = r#"SELECT DISTINCT ?other ?sr WHERE {
  <__URI__> jolux:classifiedByTaxonomyEntry ?tax .
  ?tax skos:broader ?parent .
  ?siblingTax skos:broader ?parent .
  ?other jolux:classifiedByTaxonomyEntry ?siblingTax .
  OPTIONAL { ?other jolux:historicalLegalId ?sr }
  FILTER(?other != <__URI__>)
} LIMIT __LIMIT__"#;

/// JLX-TAX-02: Findet Gesetze im selben Rechtsgebiet (Geschwister über
/// `skos:broader`).
///
/// Discovery-Funktion ohne Provenance — liefert Kandidaten für RES-03.
pub async fn find_related_by_topic(
    client: &impl SparqlClient,
    eli: &Eli,
    limit: u32,
) -> Result<Vec<RelatedLaw>, JoluxError> {
    let sparql = format!(
        "{PREFIXES}{}",
        RELATED_Q
            .replace("__URI__", &eli_uri(eli))
            .replace("__LIMIT__", &limit.to_string())
    );
    let res = client.query(&sparql).await?;
    Ok(res
        .bindings()
        .iter()
        .filter_map(|b| {
            let other = val(b, "other")?;
            Some(RelatedLaw {
                eli: other.strip_prefix(FEDLEX_BASE).unwrap_or(other).to_string(),
                sr_number: val(b, "sr").map(str::to_string),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::MockSparqlClient;
    use time::macros::date;

    #[tokio::test]
    async fn taxonomy_resolves_label_and_parent() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["tax","label","parent"]},"results":{"bindings":[
              {"tax":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/legal-taxonomy/730"},
               "label":{"type":"literal","xml:lang":"de","value":"Energie"},
               "parent":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/legal-taxonomy/7"}}
            ]}}"#,
        );
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = get_taxonomy(&client, &eli, ValidAsOf::new(date!(2026 - 01 - 01)), Language::De)
            .await
            .unwrap();
        assert_eq!(resp.data().len(), 1);
        assert_eq!(resp.data()[0].label.as_deref(), Some("Energie"));
        assert!(resp.data()[0].broader.as_deref().unwrap().ends_with("/7"));

        let q = client.last_query().unwrap();
        assert!(q.contains("jolux:classifiedByTaxonomyEntry"));
        assert!(q.contains("skos:broader"));
        assert!(q.contains(r#"LANG(?label) = "de""#));
    }

    #[tokio::test]
    async fn related_laws_exclude_self_and_strip_host() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["other","sr"]},"results":{"bindings":[
              {"other":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/1998/3033"},
               "sr":{"type":"literal","value":"730.01"}}
            ]}}"#,
        );
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let related = find_related_by_topic(&client, &eli, 10).await.unwrap();
        assert_eq!(related[0].eli, "eli/cc/1998/3033");
        assert_eq!(related[0].sr_number.as_deref(), Some("730.01"));

        let q = client.last_query().unwrap();
        assert!(q.contains("FILTER(?other != <https://fedlex.data.admin.ch/eli/cc/2017/762>)"));
        assert!(q.contains("LIMIT 10"));
    }
}
