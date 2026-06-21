//! Primitive: Stichtags-Auflösung der konsolidierten Fassung (Rulebook J2/J14).
//!
//! Der Kern der Bi-Temporalität: zu einem Stichtag die gültige Consolidation
//! (Versions-Fassung) finden und ihre XML-Manifestation liefern. Vermeidet den
//! Fehler „immer die neueste Fassung" (J3.1/J20.2).

use crate::client::{Language, PREFIXES, SparqlClient, val};
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

/// Eine Fassung in der Versionsliste eines Erlasses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    /// URI des Consolidation-Knotens.
    pub consolidation_uri: String,
    /// Stand-Datum der Fassung.
    pub date_applicability: String,
}

const VERSIONS_Q: &str = r#"SELECT DISTINCT ?cons ?date WHERE {
  ?cons jolux:isMemberOf <__URI__> ;
        jolux:dateApplicability ?date .
} ORDER BY ?date LIMIT 200"#;

/// JLX-TMP-01: Listet alle Fassungen (Consolidations) eines Erlasses, chronologisch.
///
/// Versionsanzahl ist stark typabhängig (Bundesgesetz Ø 12.3, max 118, J14.1b).
/// 6'532 CAs haben gar keine Consolidations (J3.3) — dann leere Liste, kein Fehler.
pub async fn list_versions(
    client: &impl SparqlClient,
    eli: &Eli,
    as_of: ValidAsOf,
) -> Result<Response<Vec<Version>>, JoluxError> {
    let sparql = format!("{PREFIXES}{}", VERSIONS_Q.replace("__URI__", &eli_uri(eli)));
    let res = client.query(&sparql).await?;
    let versions = res
        .bindings()
        .iter()
        .filter_map(|b| {
            Some(Version {
                consolidation_uri: val(b, "cons")?.to_string(),
                date_applicability: val(b, "date")?.to_string(),
            })
        })
        .collect();
    let prov = Provenance::new(eli.clone(), as_of, TransactionTime::now());
    Ok(Response::new(versions, prov))
}

/// Ergebnis der Geltungsprüfung eines Erlasses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InForce {
    /// Geltung zum Stichtag (Doppel-Logik, siehe [`check_in_force`]).
    pub in_force: bool,
    /// `jolux:inForceStatus` (opake Vokabular-URI), sofern vorhanden.
    pub status_uri: Option<String>,
    /// Inkrafttreten.
    pub date_entry_in_force: Option<String>,
    /// Ausserkrafttreten (deckt 96 % der Abgelaufenen, J3.2).
    pub date_no_longer_in_force: Option<String>,
    /// Ende der Anwendbarkeit (Sonderfälle, 4 %).
    pub date_end_applicability: Option<String>,
}

const IN_FORCE_Q: &str = r#"SELECT ?status ?entry ?noLonger ?endApp WHERE {
  OPTIONAL { <__URI__> jolux:inForceStatus ?status }
  OPTIONAL { <__URI__> jolux:dateEntryInForce ?entry }
  OPTIONAL { <__URI__> jolux:dateNoLongerInForce ?noLonger }
  OPTIONAL { <__URI__> jolux:dateEndApplicability ?endApp }
} LIMIT 1"#;

/// JLX-TMP-03: Prüft, ob ein Erlass zum Stichtag gilt.
///
/// Das Status-Feld allein genügt **nicht** — 15.1 % der CAs haben keinen
/// `inForceStatus`, 10'479 davon aber ein `dateEntryInForce` (J3.3). Deshalb
/// Doppel-Logik nach J3.2: primär über die Datumsfelder
/// (`entry <= as_of < min(noLonger, endApplicability)`), Fallback auf das
/// Status-Vokabular (`.../0` = in Kraft), wenn Daten fehlen.
pub async fn check_in_force(
    client: &impl SparqlClient,
    eli: &Eli,
    as_of: ValidAsOf,
) -> Result<Response<InForce>, JoluxError> {
    let uri = eli_uri(eli);
    let sparql = format!("{PREFIXES}{}", IN_FORCE_Q.replace("__URI__", &uri));
    let res = client.query(&sparql).await?;
    let b = res
        .bindings()
        .first()
        .ok_or_else(|| JoluxError::NotFound(uri.clone()))?;

    let status_uri = val(b, "status").map(str::to_string);
    let entry = val(b, "entry").map(str::to_string);
    let no_longer = val(b, "noLonger").map(str::to_string);
    let end_app = val(b, "endApp").map(str::to_string);

    // ISO-Datums-Strings vergleichen lexikografisch korrekt.
    let day = as_of.to_string();
    let started = entry.as_deref().is_some_and(|d| d <= day.as_str());
    let ended = [no_longer.as_deref(), end_app.as_deref()]
        .into_iter()
        .flatten()
        .any(|d| d <= day.as_str());
    let in_force = if entry.is_some() {
        started && !ended
    } else {
        // Fallback J3.3: kein Datum -> Status-Vokabular (0 = in Kraft).
        status_uri.as_deref().is_some_and(|s| s.ends_with("/0"))
    };

    let data = InForce {
        in_force,
        status_uri,
        date_entry_in_force: entry,
        date_no_longer_in_force: no_longer,
        date_end_applicability: end_app,
    };
    let prov = Provenance::new(eli.clone(), as_of, TransactionTime::now());
    Ok(Response::new(data, prov))
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

    #[tokio::test]
    async fn versions_are_listed_chronologically() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["cons","date"]},"results":{"bindings":[
              {"cons":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2017/762/20180101"},
               "date":{"type":"literal","value":"2018-01-01"}},
              {"cons":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2017/762/20230601"},
               "date":{"type":"literal","value":"2023-06-01"}}
            ]}}"#,
        );
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = list_versions(&client, &eli, ValidAsOf::new(date!(2026 - 01 - 01)))
            .await
            .unwrap();
        assert_eq!(resp.data().len(), 2);
        assert_eq!(resp.data()[0].date_applicability, "2018-01-01");

        let q = client.last_query().unwrap();
        assert!(q.contains("jolux:isMemberOf"));
        assert!(q.contains("ORDER BY ?date"));
    }

    #[tokio::test]
    async fn in_force_uses_date_double_logic_not_just_status() {
        // Status sagt "in Kraft" (/0), aber dateNoLongerInForce liegt vor dem
        // Stichtag -> Datumslogik gewinnt (J3.2).
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["status","entry","noLonger","endApp"]},"results":{"bindings":[{
              "status":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/enforcement-status/0"},
              "entry":{"type":"literal","value":"2000-01-01"},
              "noLonger":{"type":"literal","value":"2010-01-01"}
            }]}}"#,
        );
        let eli = Eli::new("eli/cc/1999/27").unwrap();
        let resp = check_in_force(&client, &eli, ValidAsOf::new(date!(2020 - 01 - 01)))
            .await
            .unwrap();
        assert!(!resp.data().in_force, "Datumslogik muss Status übersteuern");
        assert_eq!(
            resp.data().date_no_longer_in_force.as_deref(),
            Some("2010-01-01")
        );
    }

    #[tokio::test]
    async fn in_force_falls_back_to_status_without_dates() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["status","entry","noLonger","endApp"]},"results":{"bindings":[{
              "status":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/enforcement-status/0"}
            }]}}"#,
        );
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = check_in_force(&client, &eli, ValidAsOf::new(date!(2026 - 01 - 01)))
            .await
            .unwrap();
        assert!(resp.data().in_force, "Fallback auf Status-Vokabular (J3.3)");
    }
}
