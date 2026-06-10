//! Primitive: Struktur-Referenzen aus dem Graphen (Lexikon JLX-SUB-01/02,
//! Rulebook J4/J18.2).
//!
//! **Lückenkatalog, kein Inhaltsverzeichnis.** JOLux kennt nur 0.4–8.5 % der
//! XML-eIds — nur Elemente mit mindestens einem Impact existieren als
//! Subdivision (J4.1). Vollstruktur liefert ausschliesslich der AKN-Layer.

use crate::client::{val, SparqlClient, PREFIXES};
use crate::{eli_uri, error::JoluxError};
use fedlex_core::{Eli, Provenance, Response, TransactionTime, ValidAsOf};
use serde::{Deserialize, Serialize};

/// Eine im Graphen bekannte Untergliederung (Artikel, Kapitel, Anhang …).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subdivision {
    /// URI der Subdivision.
    pub uri: String,
    /// Subdivision-Typ (opake Vokabular-URI `subdivision-type/...`), sofern vorhanden.
    pub subdivision_type: Option<String>,
}

const SUBS_Q: &str = r#"SELECT DISTINCT ?sub ?type WHERE {
  ?sub jolux:legalResourceSubdivisionIsPartOf+ <__URI__> .
  OPTIONAL { ?sub jolux:legalResourceSubdivisionType ?type }
__TYPEFILTER__} LIMIT 500"#;

/// JLX-SUB-01: Listet die im Graphen bekannten Untergliederungen eines Erlasses.
///
/// Transitiv über `legalResourceSubdivisionIsPartOf+` (J17.3). Optional auf
/// einen Typ filterbar (`type_uri`, z.B. `.../subdivision-type/article`).
/// Leere Liste ist normal — Abdeckung sinkt mit dem Alter des Gesetzes
/// (BV 0.4 %, J4.1).
pub async fn get_subdivisions(
    client: &impl SparqlClient,
    eli: &Eli,
    as_of: ValidAsOf,
    type_uri: Option<&str>,
) -> Result<Response<Vec<Subdivision>>, JoluxError> {
    let type_filter = match type_uri {
        Some(t) => {
            let safe = t.replace(['<', '>', '"', '\\', ' '], "");
            format!("  ?sub jolux:legalResourceSubdivisionType <{safe}> .\n")
        }
        None => String::new(),
    };
    let sparql = format!(
        "{PREFIXES}{}",
        SUBS_Q
            .replace("__URI__", &eli_uri(eli))
            .replace("__TYPEFILTER__", &type_filter)
    );
    let res = client.query(&sparql).await?;
    let subs = res
        .bindings()
        .iter()
        .filter_map(|b| {
            Some(Subdivision {
                uri: val(b, "sub")?.to_string(),
                subdivision_type: val(b, "type").map(str::to_string),
            })
        })
        .collect();
    let prov = Provenance::new(eli.clone(), as_of, TransactionTime::now());
    Ok(Response::new(subs, prov))
}

/// Vokabular-URI des Subdivision-Typs `annex`.
pub const ANNEX_TYPE_URI: &str = "https://fedlex.data.admin.ch/vocabulary/subdivision-type/annex";

/// JLX-SUB-02: Listet die Anhänge eines Erlasses (Subdivision-Typ `annex`).
///
/// Spezialfall von [`get_subdivisions`] mit eigenem Lexikon-Eintrag, weil das
/// AKN-Mapping abweicht — Annexe erscheinen im XML als `<component>`, nicht
/// `<attachment>` (J18.2b). Nur ~500 CAs haben Annexe; leere Liste ist normal.
pub async fn list_annexes(
    client: &impl SparqlClient,
    eli: &Eli,
    as_of: ValidAsOf,
) -> Result<Response<Vec<Subdivision>>, JoluxError> {
    get_subdivisions(client, eli, as_of, Some(ANNEX_TYPE_URI)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::MockSparqlClient;
    use time::macros::date;

    const FIXTURE: &str = r#"{
      "head": {"vars": ["sub","type"]},
      "results": {"bindings": [
        {"sub":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2017/762/art_14"},
         "type":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/subdivision-type/article"}},
        {"sub":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2017/762/art_19"}}
      ]}
    }"#;

    #[tokio::test]
    async fn lists_subdivisions_transitively() {
        let client = MockSparqlClient::from_json(FIXTURE);
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = get_subdivisions(&client, &eli, ValidAsOf::new(date!(2026 - 01 - 01)), None)
            .await
            .unwrap();
        assert_eq!(resp.data().len(), 2);
        assert!(resp.data()[0]
            .subdivision_type
            .as_deref()
            .unwrap()
            .ends_with("/article"));
        assert!(resp.data()[1].subdivision_type.is_none());

        let q = client.last_query().unwrap();
        assert!(
            q.contains("legalResourceSubdivisionIsPartOf+"),
            "Transitiv-Pfad (J17.3) fehlt"
        );
    }

    #[tokio::test]
    async fn annexes_filter_on_annex_type() {
        let client = MockSparqlClient::from_json(FIXTURE);
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let _ = list_annexes(&client, &eli, ValidAsOf::new(date!(2026 - 01 - 01)))
            .await
            .unwrap();
        let q = client.last_query().unwrap();
        assert!(q.contains("subdivision-type/annex"));
    }

    #[tokio::test]
    async fn empty_subdivisions_is_normal_not_error() {
        let client =
            MockSparqlClient::from_json(r#"{"head":{"vars":["sub"]},"results":{"bindings":[]}}"#);
        let eli = Eli::new("eli/cc/1999/404").unwrap();
        let resp = get_subdivisions(&client, &eli, ValidAsOf::new(date!(2026 - 01 - 01)), None)
            .await
            .unwrap();
        assert!(resp.data().is_empty());
    }
}
