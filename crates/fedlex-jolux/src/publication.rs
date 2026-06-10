//! Primitive: Publikations-Lebenszyklus — OC-Grundakt, Memorial, FGA
//! (Lexikon JLX-PUB-01/02/03, Rulebook J8/J9/J19).
//!
//! **Die CC ist nicht rechtsverbindlich — die OC ist es** (J19.2). Genre und
//! Autor liegen nur auf OC-Ebene (J8.3), die CA-Felder sind leer (J1.2).

use crate::client::{val, SparqlClient, PREFIXES};
use crate::{eli_uri, error::JoluxError};
use fedlex_core::{Eli, Provenance, Response, TransactionTime, ValidAsOf};
use serde::{Deserialize, Serialize};

/// Der rechtsverbindliche Grundakt (OC) eines CC-Eintrags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcAct {
    /// URI des OC-Acts.
    pub oc_uri: String,
    /// Publikationsdatum in der AS, sofern vorhanden.
    pub publication_date: Option<String>,
    /// Genre (opake Vokabular-URI, 99.6 % befüllt, J8.3).
    pub genre: Option<String>,
    /// Federführendes Amt (`responsibilityOf`, 47.4 % befüllt).
    pub responsible_office: Option<String>,
    /// Memorial (AS-Wochenbulletin), in dem der Akt publiziert wurde.
    pub memorial: Option<String>,
}

const OC_Q: &str = r#"SELECT ?oc ?pub ?genre ?resp ?memorial WHERE {
  <__URI__> jolux:basicAct ?oc .
  OPTIONAL { ?oc jolux:publicationDate ?pub }
  OPTIONAL { ?oc jolux:legalResourceGenre ?genre }
  OPTIONAL { ?oc jolux:responsibilityOf ?resp }
  OPTIONAL { ?oc jolux:isPartOf ?memorial }
} LIMIT 1"#;

/// JLX-PUB-01: Löst den rechtsverbindlichen OC-Grundakt eines CC-Eintrags auf.
///
/// Einstieg über `jolux:basicAct` (J19.2). Genre/Autor hier abfragen, nicht
/// auf der CA (dort 0/69'350, J1.2). [`JoluxError::NotFound`], wenn der
/// CC-Eintrag keinen `basicAct` trägt.
pub async fn get_oc_act(
    client: &impl SparqlClient,
    eli: &Eli,
    as_of: ValidAsOf,
) -> Result<Response<OcAct>, JoluxError> {
    let uri = eli_uri(eli);
    let sparql = format!("{PREFIXES}{}", OC_Q.replace("__URI__", &uri));
    let res = client.query(&sparql).await?;
    let b = res
        .bindings()
        .first()
        .ok_or_else(|| JoluxError::NotFound(uri.clone()))?;
    let act = OcAct {
        oc_uri: val(b, "oc").unwrap_or_default().to_string(),
        publication_date: val(b, "pub").map(str::to_string),
        genre: val(b, "genre").map(str::to_string),
        responsible_office: val(b, "resp").map(str::to_string),
        memorial: val(b, "memorial").map(str::to_string),
    };
    let prov = Provenance::new(eli.clone(), as_of, TransactionTime::now());
    Ok(Response::new(act, prov))
}

/// Ein Memorial (AS/BBl-Wochenbulletin) mit seinen Erlassen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorialInfo {
    /// URI des Memorials (`eli/collection/{oc|fga}/YYYY/NN`, J19.3).
    pub uri: String,
    /// Die im Bulletin publizierten Akte.
    pub acts: Vec<String>,
}

const MEMORIAL_Q: &str = r#"SELECT ?m ?act WHERE {
  <__OC__> jolux:isPartOf ?m .
  ?act jolux:isPartOf ?m .
} LIMIT __LIMIT__"#;

/// JLX-PUB-02: Findet das Wochenbulletin eines OC-Acts und listet dessen Akte.
///
/// Enumerate-Richtung über inverses `isPartOf`. Reiner Publikations-Helfer
/// ohne Provenance (Aussage über ein Bulletin, nicht über eine Norm).
pub async fn get_memorial(
    client: &impl SparqlClient,
    oc_eli: &Eli,
    limit: u32,
) -> Result<MemorialInfo, JoluxError> {
    let uri = eli_uri(oc_eli);
    let sparql = format!(
        "{PREFIXES}{}",
        MEMORIAL_Q
            .replace("__OC__", &uri)
            .replace("__LIMIT__", &limit.to_string())
    );
    let res = client.query(&sparql).await?;
    let first = res
        .bindings()
        .first()
        .ok_or_else(|| JoluxError::NotFound(uri.clone()))?;
    let m_uri = val(first, "m").unwrap_or_default().to_string();
    let mut acts: Vec<String> = Vec::new();
    for b in res.bindings() {
        if let Some(act) = val(b, "act") {
            if !acts.iter().any(|a| a == act) {
                acts.push(act.to_string());
            }
        }
    }
    Ok(MemorialInfo { uri: m_uri, acts })
}

/// Ein FGA-Kontextdokument (Botschaft, Bericht …).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FgaDocument {
    /// URI des FGA-Dokuments.
    pub uri: String,
    /// Genre (opake Vokabular-URI), sofern vorhanden.
    pub genre: Option<String>,
    /// Publikationsdatum, sofern vorhanden.
    pub publication_date: Option<String>,
}

const FGA_Q: &str = r#"SELECT DISTINCT ?fga ?genre ?pub WHERE {
  <__URI__> jolux:basicAct ?oc .
  ?draft jolux:hasResultingLegalResource ?oc ;
         jolux:hasResultingLegalResource ?fga .
  FILTER(CONTAINS(STR(?fga), "/eli/fga/"))
  OPTIONAL { ?fga jolux:legalResourceGenre ?genre }
  OPTIONAL { ?fga jolux:publicationDate ?pub }
} LIMIT 50"#;

/// JLX-PUB-03: Findet die FGA-Dokumente (Botschaften/Berichte) zu einem Gesetz.
///
/// Komposition über den Draft: derselbe Draft verlinkt via
/// `hasResultingLegalResource` auf den OC-Erlass **und** die FGA-Dokumente
/// (live verifiziert 2026-06-10 am EnG-Draft `eli/proj/2012/1295`).
/// Beantwortet "Warum wurde dieses Gesetz erlassen?" (J9.2). FGA-Einträge
/// haben nie `inForceStatus` oder SR-Nummern (J9.3).
pub async fn get_fga_documents(
    client: &impl SparqlClient,
    eli: &Eli,
    as_of: ValidAsOf,
) -> Result<Response<Vec<FgaDocument>>, JoluxError> {
    let sparql = format!("{PREFIXES}{}", FGA_Q.replace("__URI__", &eli_uri(eli)));
    let res = client.query(&sparql).await?;
    let docs = res
        .bindings()
        .iter()
        .filter_map(|b| {
            Some(FgaDocument {
                uri: val(b, "fga")?.to_string(),
                genre: val(b, "genre").map(str::to_string),
                publication_date: val(b, "pub").map(str::to_string),
            })
        })
        .collect();
    let prov = Provenance::new(eli.clone(), as_of, TransactionTime::now());
    Ok(Response::new(docs, prov))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::MockSparqlClient;
    use time::macros::date;

    #[tokio::test]
    async fn oc_act_carries_genre_and_memorial() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["oc","pub","genre","resp","memorial"]},"results":{"bindings":[{
              "oc":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/oc/2017/762"},
              "pub":{"type":"literal","value":"2017-11-21"},
              "genre":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/resource-genre/erlasstexte"},
              "memorial":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/collection/oc/2017/105"}
            }]}}"#,
        );
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = get_oc_act(&client, &eli, ValidAsOf::new(date!(2026 - 01 - 01)))
            .await
            .unwrap();
        assert!(resp.data().oc_uri.ends_with("eli/oc/2017/762"));
        assert!(
            resp.data().genre.is_some(),
            "Genre liegt auf OC-Ebene (J8.3)"
        );
        assert!(resp
            .data()
            .memorial
            .as_deref()
            .unwrap()
            .contains("eli/collection/"));

        let q = client.last_query().unwrap();
        assert!(q.contains("jolux:basicAct"));
    }

    #[tokio::test]
    async fn missing_basic_act_is_not_found() {
        let client =
            MockSparqlClient::from_json(r#"{"head":{"vars":["oc"]},"results":{"bindings":[]}}"#);
        let eli = Eli::new("eli/cc/1900/1").unwrap();
        let err = get_oc_act(&client, &eli, ValidAsOf::new(date!(2026 - 01 - 01)))
            .await
            .unwrap_err();
        assert!(matches!(err, JoluxError::NotFound(_)));
    }

    #[tokio::test]
    async fn memorial_lists_unique_acts() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["m","act"]},"results":{"bindings":[
              {"m":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/collection/oc/2017/105"},
               "act":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/oc/2017/762"}},
              {"m":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/collection/oc/2017/105"},
               "act":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/oc/2017/762"}},
              {"m":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/collection/oc/2017/105"},
               "act":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/oc/2017/763"}}
            ]}}"#,
        );
        let oc = Eli::new("eli/oc/2017/762").unwrap();
        let info = get_memorial(&client, &oc, 100).await.unwrap();
        assert!(info.uri.contains("eli/collection/oc/2017/105"));
        assert_eq!(info.acts.len(), 2, "Quad-Store-Duplikate nicht entfernt");
    }

    #[tokio::test]
    async fn fga_documents_compose_via_draft() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["fga","genre","pub"]},"results":{"bindings":[
              {"fga":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/fga/2016/1642"},
               "genre":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/resource-genre/botschaft"}}
            ]}}"#,
        );
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = get_fga_documents(&client, &eli, ValidAsOf::new(date!(2026 - 01 - 01)))
            .await
            .unwrap();
        assert_eq!(resp.data().len(), 1);
        assert!(resp.data()[0].uri.contains("eli/fga/2016/1642"));

        let q = client.last_query().unwrap();
        assert!(q.contains("jolux:hasResultingLegalResource"));
        assert!(q.contains(r#"CONTAINS(STR(?fga), "/eli/fga/")"#));
    }
}
