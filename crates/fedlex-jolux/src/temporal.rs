//! Primitive: Stichtags-Auflösung der konsolidierten Fassung (Rulebook J2/J14).
//!
//! Der Kern der Bi-Temporalität: zu einem Stichtag die gültige Consolidation
//! (Versions-Fassung) finden und ihre XML-Manifestation liefern. Vermeidet den
//! Fehler „immer die neueste Fassung" (J3.1/J20.2).

use crate::client::{val, Language, SparqlClient, PREFIXES};
use crate::{eli_uri, error::JoluxError};
use fedlex_core::{Eli, Provenance, Response, TransactionTime, ValidAsOf};
use serde::{Deserialize, Serialize};

/// Eine zum Stichtag gültige konsolidierte Fassung eines Erlasses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Consolidation {
    /// URI des Consolidation-Knotens.
    pub consolidation_uri: String,
    /// Stand-Datum der Fassung (`jolux:dateApplicability`).
    pub date_applicability: String,
    /// Download-URL der AKN-XML-Manifestation.
    pub xml_url: String,
    /// Sprache der Manifestation.
    pub language: String,
}

const CONS_Q: &str = r#"SELECT ?cons ?date ?url WHERE {
  ?cons jolux:isMemberOf <__URI__> ;
        jolux:dateApplicability ?date ;
        jolux:isRealizedBy ?expr .
  ?expr jolux:language <__LANGURI__> ;
        jolux:isEmbodiedBy ?manif .
  ?manif jolux:isExemplifiedBy ?url .
  FILTER(CONTAINS(STR(?url), "xml"))
  FILTER(?date <= "__DATE__"^^xsd:date)
} ORDER BY DESC(?date) LIMIT 1"#;

/// Findet die zum Stichtag `as_of` gültige konsolidierte Fassung + ihre XML-URL.
///
/// Filtert über `dateApplicability <= as_of` und nimmt die jüngste (Rulebook
/// J14.3). Liefert [`JoluxError::NotFound`], wenn es zum Stichtag keine Fassung
/// gibt (z.B. vor Inkrafttreten).
pub async fn resolve_consolidation_at(
    client: &impl SparqlClient,
    eli: &Eli,
    as_of: ValidAsOf,
    lang: Language,
) -> Result<Response<Consolidation>, JoluxError> {
    let uri = eli_uri(eli);
    let sparql = format!(
        "{PREFIXES}{}",
        CONS_Q
            .replace("__URI__", &uri)
            .replace("__LANGURI__", lang.vocab_uri())
            .replace("__DATE__", &as_of.to_string())
    );
    let res = client.query(&sparql).await?;
    let b = res
        .bindings()
        .first()
        .ok_or_else(|| JoluxError::NotFound(uri.clone()))?;

    let cons = Consolidation {
        consolidation_uri: val(b, "cons").unwrap_or_default().to_string(),
        date_applicability: val(b, "date").unwrap_or_default().to_string(),
        xml_url: val(b, "url").unwrap_or_default().to_string(),
        language: lang.tag().to_string(),
    };

    let prov = Provenance::new(eli.clone(), as_of, TransactionTime::now());
    Ok(Response::new(cons, prov))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::MockSparqlClient;
    use time::macros::date;

    const FIXTURE: &str = r#"{
      "head": {"vars": ["cons","date","url"]},
      "results": {"bindings": [{
        "cons": {"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2017/762/consolidation/20230601"},
        "date": {"type":"literal","value":"2023-06-01"},
        "url": {"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2017/762/20230601/de/xml/fedlex-data-admin-ch-eli-cc-2017-762-20230601-de-xml.xml"}
      }]}
    }"#;

    #[tokio::test]
    async fn resolves_version_at_stichtag_with_filter_and_language() {
        let client = MockSparqlClient::from_json(FIXTURE);
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = resolve_consolidation_at(
            &client,
            &eli,
            ValidAsOf::new(date!(2023 - 06 - 15)),
            Language::De,
        )
        .await
        .unwrap();

        assert_eq!(resp.data().date_applicability, "2023-06-01");
        assert!(resp.data().xml_url.contains("/xml/"));
        assert_eq!(resp.data().language, "de");
        assert_eq!(resp.provenance().valid_as_of.to_string(), "2023-06-15");

        // Stichtags-Filter + Sprachvokabular landen korrekt in der Query.
        let q = client.last_query().unwrap();
        assert!(q.contains(r#"FILTER(?date <= "2023-06-15"^^xsd:date)"#));
        assert!(q.contains("/DEU"));
        assert!(q.contains("ORDER BY DESC(?date)"));
    }

    #[tokio::test]
    async fn no_version_before_entry_into_force_is_not_found() {
        let empty = r#"{"head":{"vars":["cons","date","url"]},"results":{"bindings":[]}}"#;
        let client = MockSparqlClient::from_json(empty);
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let err = resolve_consolidation_at(
            &client,
            &eli,
            ValidAsOf::new(date!(1990 - 01 - 01)),
            Language::De,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, JoluxError::NotFound(_)));
    }
}
