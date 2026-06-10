//! Primitive: Völkerrecht — Staatsverträge (Lexikon JLX-TRT-01/02,
//! Rulebook J12).

use crate::client::{val, SparqlClient, PREFIXES};
use crate::error::JoluxError;
use serde::{Deserialize, Serialize};

/// Informationen zu einem Vertragsprozess (`jolux:TreatyProcess`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreatyInfo {
    /// URI des TreatyProcess (`eli/treaty/YYYY/NNNN`).
    pub process_uri: String,
    /// Vertragstitel, sofern vorhanden.
    pub title: Option<String>,
    /// Unterzeichnungsdatum.
    pub signature_date: Option<String>,
    /// Unterzeichnungsort.
    pub signature_place: Option<String>,
    /// Bilateral-Flag (1.6 % der Prozesse ohne Flag, J12.2).
    pub bilateral: Option<bool>,
    /// Vertragsparteien (Länder-Vokabular-URIs).
    pub party_countries: Vec<String>,
    /// Genehmigungs-Bundesbeschluss (`approbationAct`), sofern vorhanden.
    pub approbation_act: Option<String>,
}

const TREATY_Q: &str = r#"SELECT ?title ?sigDate ?sigPlace ?bilateral ?country ?approbation WHERE {
  OPTIONAL { <__URI__> jolux:titleTreaty ?title }
  OPTIONAL { <__URI__> jolux:treatySignatureDate ?sigDate }
  OPTIONAL { <__URI__> jolux:treatySignaturePlace ?sigPlace }
  OPTIONAL { <__URI__> jolux:bilateral ?bilateral }
  OPTIONAL { <__URI__> jolux:treatyPartyCountry ?country }
  OPTIONAL { <__URI__> jolux:approbationAct ?approbation }
} LIMIT 100"#;

/// JLX-TRT-01: Steckbrief eines Staatsvertrags-Prozesses.
///
/// Einstieg über die TreatyProcess-URI (`eli/treaty/...`). Der
/// `approbation_act` verknüpft zum Genehmigungs-Bundesbeschluss — eine
/// eigene Erlass-Kette (J12). Liefert [`JoluxError::NotFound`], wenn der
/// Knoten keine Treaty-Prädikate trägt.
pub async fn get_treaty_info(
    client: &impl SparqlClient,
    process_uri: &str,
) -> Result<TreatyInfo, JoluxError> {
    let safe = process_uri.replace(['<', '>', '"', '\\', ' '], "");
    let sparql = format!("{PREFIXES}{}", TREATY_Q.replace("__URI__", &safe));
    let res = client.query(&sparql).await?;
    if res.is_empty() {
        return Err(JoluxError::NotFound(safe));
    }

    let first = &res.bindings()[0];
    let mut info = TreatyInfo {
        process_uri: safe,
        title: val(first, "title").map(str::to_string),
        signature_date: val(first, "sigDate").map(str::to_string),
        signature_place: val(first, "sigPlace").map(str::to_string),
        bilateral: val(first, "bilateral").map(|b| b == "true" || b == "1"),
        party_countries: Vec::new(),
        approbation_act: val(first, "approbation").map(str::to_string),
    };
    for b in res.bindings() {
        if let Some(c) = val(b, "country") {
            if !info.party_countries.iter().any(|x| x == c) {
                info.party_countries.push(c.to_string());
            }
        }
    }
    Ok(info)
}

/// Ein Treffer der Vertrags-Suche.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreatyHit {
    /// URI des TreatyProcess.
    pub process_uri: String,
    /// Unterzeichnungsdatum, sofern vorhanden.
    pub signature_date: Option<String>,
}

const FIND_TREATIES_Q: &str = r#"SELECT DISTINCT ?process ?sigDate WHERE {
  ?process a jolux:TreatyProcess .
__FILTERS__  OPTIONAL { ?process jolux:treatySignatureDate ?sigDate }
} ORDER BY DESC(?sigDate) LIMIT __LIMIT__"#;

/// JLX-TRT-02: Findet Vertragsprozesse nach Land und/oder Bilateral-Flag.
///
/// Enumerate über `treatyPartyCountry` (Vokabular `country`, 429 Einträge)
/// und `bilateral` (J12.2/J12.3). Mindestens ein Filter ist sinnvoll —
/// ohne Filter werden schlicht die jüngsten Prozesse geliefert.
pub async fn find_treaties(
    client: &impl SparqlClient,
    country_uri: Option<&str>,
    bilateral: Option<bool>,
    limit: u32,
) -> Result<Vec<TreatyHit>, JoluxError> {
    let mut filters = String::new();
    if let Some(c) = country_uri {
        let safe = c.replace(['<', '>', '"', '\\', ' '], "");
        filters.push_str(&format!("  ?process jolux:treatyPartyCountry <{safe}> .\n"));
    }
    if let Some(b) = bilateral {
        filters.push_str(&format!("  ?process jolux:bilateral {b} .\n"));
    }
    let sparql = format!(
        "{PREFIXES}{}",
        FIND_TREATIES_Q
            .replace("__FILTERS__", &filters)
            .replace("__LIMIT__", &limit.to_string())
    );
    let res = client.query(&sparql).await?;
    Ok(res
        .bindings()
        .iter()
        .filter_map(|b| {
            Some(TreatyHit {
                process_uri: val(b, "process")?.to_string(),
                signature_date: val(b, "sigDate").map(str::to_string),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::MockSparqlClient;

    #[tokio::test]
    async fn treaty_info_aggregates_countries() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["title","sigDate","sigPlace","bilateral","country","approbation"]},
            "results":{"bindings":[
              {"title":{"type":"literal","value":"Abkommen X"},
               "sigDate":{"type":"literal","value":"1999-06-21"},
               "bilateral":{"type":"literal","value":"true"},
               "country":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/country/136"}},
              {"title":{"type":"literal","value":"Abkommen X"},
               "sigDate":{"type":"literal","value":"1999-06-21"},
               "bilateral":{"type":"literal","value":"true"},
               "country":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/country/336"}}
            ]}}"#,
        );
        let info = get_treaty_info(&client, "https://fedlex.data.admin.ch/eli/treaty/1999/0001")
            .await
            .unwrap();
        assert_eq!(info.bilateral, Some(true));
        assert_eq!(info.party_countries.len(), 2);
        assert_eq!(info.signature_date.as_deref(), Some("1999-06-21"));
    }

    #[tokio::test]
    async fn unknown_process_is_not_found() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["title"]},"results":{"bindings":[]}}"#,
        );
        let err = get_treaty_info(&client, "https://fedlex.data.admin.ch/eli/treaty/0/0")
            .await
            .unwrap_err();
        assert!(matches!(err, JoluxError::NotFound(_)));
    }

    #[tokio::test]
    async fn find_treaties_builds_filters() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["process","sigDate"]},"results":{"bindings":[
              {"process":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/treaty/1852/0001"},
               "sigDate":{"type":"literal","value":"1852-07-01"}}
            ]}}"#,
        );
        let hits = find_treaties(
            &client,
            Some("https://fedlex.data.admin.ch/vocabulary/country/136"),
            Some(true),
            5,
        )
        .await
        .unwrap();
        assert_eq!(hits.len(), 1);

        let q = client.last_query().unwrap();
        assert!(q.contains("jolux:treatyPartyCountry <https://fedlex.data.admin.ch/vocabulary/country/136>"));
        assert!(q.contains("jolux:bilateral true"));
        assert!(q.contains("LIMIT 5"));
    }
}
