//! Primitive: Identitäts-Auflösung — SR-Nummer, Sprachen, Manifestationen
//! (Lexikon JLX-RES-01/04/05, Rulebook J2/J13).

use crate::client::{Language, PREFIXES, SparqlClient, val};
use crate::{FEDLEX_BASE, eli_uri, error::JoluxError};
use fedlex_core::{Eli, Provenance, Response, TransactionTime, ValidAsOf};
use serde::{Deserialize, Serialize};

/// Ein Treffer der SR-Nummern-Auflösung.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SrHit {
    /// ELI des Erlasses (relativ, `eli/cc/...`).
    pub eli: String,
    /// Titel in der angefragten Sprache, sofern vorhanden.
    pub title: Option<String>,
    /// `jolux:inForceStatus` (opake Vokabular-URI), sofern vorhanden.
    pub in_force_status: Option<String>,
}

const SR_Q: &str = r#"SELECT DISTINCT ?ca ?title ?status WHERE {
  ?ca a jolux:ConsolidationAbstract ;
      jolux:historicalLegalId "__SR__" .
  OPTIONAL {
    ?ca jolux:isRealizedBy ?expr .
    ?expr jolux:language <__LANGURI__> ; jolux:title ?title .
  }
  OPTIONAL { ?ca jolux:inForceStatus ?status }
} LIMIT 20"#;

/// JLX-RES-01: Löst eine SR-Nummer zu den passenden Erlassen auf.
///
/// **Liefert eine Liste**, denn SR-Nummern werden wiederverwendet — 730.0
/// zeigt auf das alte EnG (`eli/cc/1999/27`, aufgehoben) *und* das neue
/// (`eli/cc/2017/762`). Disambiguierung über `in_force_status`
/// (`.../enforcement-status/0` = in Kraft) bzw. [`check_in_force`].
/// Live-verifiziert 2026-06-10.
///
/// Discovery-Funktion ohne Provenance. Die SR-Nummer wird entschärft
/// eingebettet (keine SPARQL-Injection).
///
/// [`check_in_force`]: crate::temporal::check_in_force
pub async fn resolve_sr_number(
    client: &impl SparqlClient,
    sr_number: &str,
    lang: Language,
) -> Result<Vec<SrHit>, JoluxError> {
    let safe = sr_number.replace(['"', '\\'], " ");
    let sparql = format!(
        "{PREFIXES}{}",
        SR_Q.replace("__SR__", &safe)
            .replace("__LANGURI__", lang.vocab_uri())
    );
    let res = client.query(&sparql).await?;
    let hits = res
        .bindings()
        .iter()
        .filter_map(|b| {
            let ca = val(b, "ca")?;
            Some(SrHit {
                eli: ca.strip_prefix(FEDLEX_BASE).unwrap_or(ca).to_string(),
                title: val(b, "title").map(str::to_string),
                in_force_status: val(b, "status").map(str::to_string),
            })
        })
        .collect();
    Ok(hits)
}

/// Gewünschtes Manifestations-Format (Rulebook J19.5: XML/PDF/HTML/DOCX).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestationFormat {
    /// AKN-XML (ab ~2021 verfügbar, J14.2).
    Xml,
    /// PDF-A (vollständige Historie).
    Pdf,
    /// HTML.
    Html,
}

impl ManifestationFormat {
    /// Teil-String, über den die Exemplar-URL gefiltert wird.
    fn url_marker(self) -> &'static str {
        match self {
            ManifestationFormat::Xml => "xml",
            ManifestationFormat::Pdf => "pdf",
            ManifestationFormat::Html => "html",
        }
    }
}

/// Eine aufgelöste Manifestation (Download-URL einer Fassung).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifestation {
    /// Download-URL des Exemplars.
    pub url: String,
    /// Stand-Datum der zugrunde liegenden Fassung.
    pub consolidation_date: String,
    /// Sprache der Manifestation.
    pub language: String,
}

const MANIF_Q: &str = r#"SELECT ?date ?url WHERE {
  ?cons jolux:isMemberOf <__URI__> ;
        jolux:dateApplicability ?date ;
        jolux:isRealizedBy ?expr .
  ?expr jolux:language <__LANGURI__> ;
        jolux:isEmbodiedBy ?manif .
  ?manif jolux:isExemplifiedBy ?url .
  FILTER(CONTAINS(STR(?url), "__FMT__"))
  FILTER(?date <= "__DATE__"^^xsd:date)
} ORDER BY DESC(?date) LIMIT 1"#;

/// JLX-RES-04: Liefert die Download-URL der zum Stichtag gültigen Fassung
/// im gewünschten Format.
///
/// FRBR-Kette `?cons isMemberOf <CA>` → `isRealizedBy` → `isEmbodiedBy` →
/// `isExemplifiedBy` (J2.1/J2.2). Die Richtung ist **eingehend** — die
/// Gegenrichtung liefert 0 Ergebnisse. XML existiert erst ab ~2021, ältere
/// Fassungen nur als PDF (J14.2) → dann [`JoluxError::NotFound`].
pub async fn resolve_manifestation(
    client: &impl SparqlClient,
    eli: &Eli,
    as_of: ValidAsOf,
    lang: Language,
    format: ManifestationFormat,
) -> Result<Response<Manifestation>, JoluxError> {
    let uri = eli_uri(eli);
    let sparql = format!(
        "{PREFIXES}{}",
        MANIF_Q
            .replace("__URI__", &uri)
            .replace("__LANGURI__", lang.vocab_uri())
            .replace("__FMT__", format.url_marker())
            .replace("__DATE__", &as_of.to_string())
    );
    let res = client.query(&sparql).await?;
    let b = res
        .bindings()
        .first()
        .ok_or_else(|| JoluxError::NotFound(uri.clone()))?;
    let manif = Manifestation {
        url: val(b, "url").unwrap_or_default().to_string(),
        consolidation_date: val(b, "date").unwrap_or_default().to_string(),
        language: lang.tag().to_string(),
    };
    let prov = Provenance::new(eli.clone(), as_of, TransactionTime::now());
    Ok(Response::new(manif, prov))
}

const EXPR_Q: &str = r#"SELECT DISTINCT ?lang WHERE {
  ?cons jolux:isMemberOf <__URI__> ;
        jolux:isRealizedBy ?expr .
  ?expr jolux:language ?lang .
} LIMIT 10"#;

/// JLX-RES-05: Listet die Sprachen, in denen ein Erlass vorliegt.
///
/// Liefert die EU-Sprachvokabular-URIs der Expressions (J13.1: DE/FR/IT
/// flächendeckend, EN 290, RM 85). Reiner Helfer ohne Provenance.
pub async fn list_expressions(
    client: &impl SparqlClient,
    eli: &Eli,
) -> Result<Vec<String>, JoluxError> {
    let sparql = format!("{PREFIXES}{}", EXPR_Q.replace("__URI__", &eli_uri(eli)));
    let res = client.query(&sparql).await?;
    Ok(res
        .bindings()
        .iter()
        .filter_map(|b| val(b, "lang").map(str::to_string))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::MockSparqlClient;
    use time::macros::date;

    #[tokio::test]
    async fn sr_number_returns_all_reused_hits() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["ca","title","status"]},"results":{"bindings":[
              {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/1999/27"},
               "status":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/enforcement-status/1"}},
              {"ca":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2017/762"},
               "title":{"type":"literal","xml:lang":"de","value":"Energiegesetz (EnG)"},
               "status":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/enforcement-status/0"}}
            ]}}"#,
        );
        let hits = resolve_sr_number(&client, "730.0", Language::De)
            .await
            .unwrap();
        assert_eq!(hits.len(), 2, "SR-Wiederverwendung: Liste, kein Einzelwert");
        assert_eq!(hits[1].eli, "eli/cc/2017/762");
        assert!(hits[1].in_force_status.as_deref().unwrap().ends_with("/0"));

        let q = client.last_query().unwrap();
        assert!(q.contains(r#"jolux:historicalLegalId "730.0""#));
        assert!(q.contains("SELECT DISTINCT"));
    }

    #[tokio::test]
    async fn sr_number_neutralizes_injection() {
        let client =
            MockSparqlClient::from_json(r#"{"head":{"vars":["ca"]},"results":{"bindings":[]}}"#);
        let _ = resolve_sr_number(&client, r#"730" } INJECT {"#, Language::De)
            .await
            .unwrap();
        let q = client.last_query().unwrap();
        assert!(!q.contains("\" }"), "Breakout-Sequenz nicht neutralisiert");
    }

    #[tokio::test]
    async fn manifestation_filters_format_and_date() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["date","url"]},"results":{"bindings":[{
              "date":{"type":"literal","value":"2023-06-01"},
              "url":{"type":"uri","value":"https://fedlex.data.admin.ch/.../de/pdf-a/doc.pdf"}
            }]}}"#,
        );
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = resolve_manifestation(
            &client,
            &eli,
            ValidAsOf::new(date!(2023 - 06 - 15)),
            Language::De,
            ManifestationFormat::Pdf,
        )
        .await
        .unwrap();
        assert!(resp.data().url.ends_with(".pdf"));
        assert_eq!(resp.data().consolidation_date, "2023-06-01");

        let q = client.last_query().unwrap();
        assert!(q.contains(r#"CONTAINS(STR(?url), "pdf")"#));
        assert!(q.contains(r#"?date <= "2023-06-15"^^xsd:date"#));
    }

    #[tokio::test]
    async fn manifestation_missing_format_is_not_found() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["date","url"]},"results":{"bindings":[]}}"#,
        );
        let eli = Eli::new("eli/cc/1907/233").unwrap();
        let err = resolve_manifestation(
            &client,
            &eli,
            ValidAsOf::new(date!(1950 - 01 - 01)),
            Language::De,
            ManifestationFormat::Xml,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, JoluxError::NotFound(_)));
    }

    #[tokio::test]
    async fn expressions_list_language_uris() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["lang"]},"results":{"bindings":[
              {"lang":{"type":"uri","value":"http://publications.europa.eu/resource/authority/language/DEU"}},
              {"lang":{"type":"uri","value":"http://publications.europa.eu/resource/authority/language/FRA"}},
              {"lang":{"type":"uri","value":"http://publications.europa.eu/resource/authority/language/ITA"}}
            ]}}"#,
        );
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let langs = list_expressions(&client, &eli).await.unwrap();
        assert_eq!(langs.len(), 3);
        assert!(langs[0].ends_with("/DEU"));
    }
}
