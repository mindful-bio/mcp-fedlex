//! Primitive: Gesetzes-Metadaten (Rulebook J1/J3).

use crate::client::{val, SparqlClient, PREFIXES};
use crate::{eli_uri, error::JoluxError};
use fedlex_core::{Eli, Provenance, Response, TransactionTime, ValidAsOf};
use serde::{Deserialize, Serialize};

/// Kern-Metadaten eines Erlasses auf ConsolidationAbstract-Ebene.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LawMetadata {
    /// ELI des Erlasses.
    pub eli: String,
    /// SR-Nummer (`jolux:historicalLegalId`), z.B. `730.0`. Oft, aber nicht immer gesetzt.
    pub sr_number: Option<String>,
    /// Volltitel (Expression-Ebene, deutsch).
    pub title: Option<String>,
    /// Erlass-Datum (`jolux:dateDocument`).
    pub date_document: Option<String>,
    /// Inkrafttreten (`jolux:dateEntryInForce`).
    pub date_entry_in_force: Option<String>,
    /// Dokumenttyp-URI (`jolux:typeDocument`, opak -> via Vocabulary auflösen).
    pub type_document: Option<String>,
}

const META_Q: &str = r#"SELECT ?sr ?title ?dateDocument ?dateEntryInForce ?typeDocument WHERE {
  OPTIONAL { <__URI__> jolux:historicalLegalId ?sr }
  OPTIONAL { <__URI__> jolux:dateDocument ?dateDocument }
  OPTIONAL { <__URI__> jolux:dateEntryInForce ?dateEntryInForce }
  OPTIONAL { <__URI__> jolux:typeDocument ?typeDocument }
  OPTIONAL {
    <__URI__> jolux:isRealizedBy ?expr .
    ?expr jolux:language <http://publications.europa.eu/resource/authority/language/DEU> ;
          jolux:title ?title .
  }
} LIMIT 1"#;

/// Holt die Kern-Metadaten eines Erlasses.
///
/// Liefert eine [`Response`] mit Provenance (ELI + `as_of`). Felder sind
/// `Option`, weil auf CA-Ebene mehrere Prädikate systematisch leer sein können
/// (Rulebook J1.2/J3.4) — `OPTIONAL` ist daher Pflicht.
///
/// **Live-verifiziert (2026-06-10):** Titel über die CA-direkte Expression
/// (`<CA> isRealizedBy`), nicht über Consolidation-Expressions (die tragen nur
/// technische Labels).
pub async fn get_law_metadata(
    client: &impl SparqlClient,
    eli: &Eli,
    as_of: ValidAsOf,
) -> Result<Response<LawMetadata>, JoluxError> {
    let uri = eli_uri(eli);
    let sparql = format!("{PREFIXES}{}", META_Q.replace("__URI__", &uri));
    let res = client.query(&sparql).await?;
    let b = res.bindings().first();

    let meta = LawMetadata {
        eli: eli.as_str().to_string(),
        sr_number: b.and_then(|b| val(b, "sr")).map(str::to_string),
        title: b.and_then(|b| val(b, "title")).map(str::to_string),
        date_document: b.and_then(|b| val(b, "dateDocument")).map(str::to_string),
        date_entry_in_force: b
            .and_then(|b| val(b, "dateEntryInForce"))
            .map(str::to_string),
        type_document: b.and_then(|b| val(b, "typeDocument")).map(str::to_string),
    };

    let prov = Provenance::new(eli.clone(), as_of, TransactionTime::now());
    Ok(Response::new(meta, prov))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::MockSparqlClient;
    use time::macros::date;

    const FIXTURE: &str = r#"{
      "head": {"vars": ["sr","title","dateDocument","dateEntryInForce","typeDocument"]},
      "results": {"bindings": [{
        "sr": {"type":"literal","value":"730.0"},
        "title": {"type":"literal","xml:lang":"de","value":"Energiegesetz vom 30. September 2016 (EnG)"},
        "dateDocument": {"type":"literal","value":"2016-09-30"},
        "dateEntryInForce": {"type":"literal","value":"2018-01-01"},
        "typeDocument": {"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/resource-type/21"}
      }]}
    }"#;

    #[tokio::test]
    async fn parses_metadata_and_carries_provenance() {
        let client = MockSparqlClient::from_json(FIXTURE);
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = get_law_metadata(&client, &eli, ValidAsOf::new(date!(2024 - 01 - 01)))
            .await
            .unwrap();

        assert_eq!(resp.data().sr_number.as_deref(), Some("730.0"));
        assert!(resp
            .data()
            .title
            .as_deref()
            .unwrap()
            .contains("Energiegesetz"));
        assert_eq!(
            resp.data().date_entry_in_force.as_deref(),
            Some("2018-01-01")
        );

        // ADR-004: Provenance trägt ELI + Stichtag.
        assert_eq!(resp.provenance().eli.as_str(), "eli/cc/2017/762");
        assert_eq!(resp.provenance().valid_as_of.to_string(), "2024-01-01");

        // Die Query expandiert den ELI zur vollen Fedlex-URI.
        let q = client.last_query().unwrap();
        assert!(q.contains("<https://fedlex.data.admin.ch/eli/cc/2017/762>"));
        assert!(q.contains("jolux:historicalLegalId"));
    }

    #[tokio::test]
    async fn missing_fields_stay_none_not_error() {
        let empty = r#"{"head":{"vars":[]},"results":{"bindings":[]}}"#;
        let client = MockSparqlClient::from_json(empty);
        let eli = Eli::new("eli/cc/1999/404").unwrap();
        let resp = get_law_metadata(&client, &eli, ValidAsOf::new(date!(2024 - 01 - 01)))
            .await
            .unwrap();
        assert_eq!(resp.data().sr_number, None);
        assert_eq!(resp.data().title, None);
        // Provenance ist trotzdem vollständig.
        assert_eq!(resp.provenance().eli.as_str(), "eli/cc/1999/404");
    }
}
