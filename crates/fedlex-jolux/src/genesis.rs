//! Primitive: Entstehungsgeschichte — Drafts, Vernehmlassungen, Dokumente
//! (Lexikon JLX-GEN-01/02/03, Rulebook J10/J11).

use crate::client::{val, SparqlClient, PREFIXES};
use crate::{eli_uri, error::JoluxError};
use fedlex_core::{Eli, Provenance, Response, TransactionTime, ValidAsOf};
use serde::{Deserialize, Serialize};

/// Ein Gesetzgebungs-Draft (parlamentarischer Prozess).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    /// URI des Draft-Knotens (`eli/proj/YYYY/NNNN`).
    pub uri: String,
    /// Interne Dossier-Nummer (`jolux:draftId`), sofern vorhanden.
    pub draft_id: Option<String>,
    /// Curia-Vista-Geschäftsnummer (`jolux:parliamentDraftId`, z.B. "13.074").
    pub parliament_draft_id: Option<String>,
    /// Resultierende Erlasse/Dokumente (`hasResultingLegalResource`).
    pub resulting_resources: Vec<String>,
}

const DRAFTS_Q: &str = r#"SELECT ?draft ?draftId ?parlId ?result WHERE {
  ?draft jolux:hasResultingLegalResource ?oc .
  <__URI__> jolux:basicAct ?oc .
  OPTIONAL { ?draft jolux:draftId ?draftId }
  OPTIONAL { ?draft jolux:parliamentDraftId ?parlId }
  OPTIONAL { ?draft jolux:hasResultingLegalResource ?result }
} LIMIT 100"#;

/// JLX-GEN-01: Findet die Drafts (Gesetzgebungsprozesse), aus denen ein
/// Gesetz hervorging.
///
/// Traverse-in über `hasResultingLegalResource` auf den OC-Grundakt (J11.2).
/// `parliament_draft_id` ist der Föderations-Schlüssel zu Curia Vista.
/// Live verifiziert 2026-06-10 am EnG (Draft `eli/proj/2012/1295`, "13.074").
pub async fn get_drafts(
    client: &impl SparqlClient,
    eli: &Eli,
    as_of: ValidAsOf,
) -> Result<Response<Vec<Draft>>, JoluxError> {
    let sparql = format!("{PREFIXES}{}", DRAFTS_Q.replace("__URI__", &eli_uri(eli)));
    let res = client.query(&sparql).await?;

    let mut drafts: Vec<Draft> = Vec::new();
    for b in res.bindings() {
        let Some(uri) = val(b, "draft") else { continue };
        let entry = match drafts.iter_mut().find(|d| d.uri == uri) {
            Some(e) => e,
            None => {
                drafts.push(Draft {
                    uri: uri.to_string(),
                    draft_id: val(b, "draftId").map(str::to_string),
                    parliament_draft_id: val(b, "parlId").map(str::to_string),
                    resulting_resources: Vec::new(),
                });
                drafts.last_mut().expect("just pushed")
            }
        };
        if let Some(r) = val(b, "result") {
            if !entry.resulting_resources.iter().any(|x| x == r) {
                entry.resulting_resources.push(r.to_string());
            }
        }
    }

    let prov = Provenance::new(eli.clone(), as_of, TransactionTime::now());
    Ok(Response::new(drafts, prov))
}

/// Eine Vernehmlassung (Consultation) mit ihren Task-Daten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Consultation {
    /// URI der Consultation (`eli/dl/proj/YYYY/ID/cons_N`, J20.4).
    pub uri: String,
    /// Status (opake Vokabular-URI).
    pub status: Option<String>,
    /// Beginn (von den Tasks aggregiert).
    pub start_date: Option<String>,
    /// Ende (von den Tasks aggregiert).
    pub end_date: Option<String>,
    /// Federführende Institution.
    pub institution: Option<String>,
}

const CONSULTATIONS_Q: &str = r#"SELECT ?cons ?status ?start ?end ?inst WHERE {
  <__DRAFT__> jolux:draftHasTask ?cons .
  ?cons a jolux:Consultation .
  OPTIONAL { ?cons jolux:consultationStatus ?status }
  OPTIONAL { ?cons jolux:hasSubTask ?task .
             OPTIONAL { ?task jolux:eventStartDate ?start }
             OPTIONAL { ?task jolux:eventEndDate ?end }
             OPTIONAL { ?task jolux:institutionInChargeOfTheEvent ?inst } }
} LIMIT 50"#;

/// JLX-GEN-02: Vernehmlassungen eines Drafts, inklusive Task-Daten.
///
/// **Falltrap J10.2:** Daten wie Start/Ende/Institution hängen **nie** direkt
/// an der Consultation, sondern an ihren Sub-Tasks (`hasSubTask` →
/// `ConsultationTask`) — die Query traversiert den Zwischenknoten. Einstieg
/// über die Draft-URI (`eli/proj/...` bzw. `eli/dl/proj/...`).
pub async fn get_consultations(
    client: &impl SparqlClient,
    draft_uri: &str,
) -> Result<Vec<Consultation>, JoluxError> {
    let safe = draft_uri.replace(['<', '>', '"', '\\', ' '], "");
    let sparql = format!("{PREFIXES}{}", CONSULTATIONS_Q.replace("__DRAFT__", &safe));
    let res = client.query(&sparql).await?;

    let mut out: Vec<Consultation> = Vec::new();
    for b in res.bindings() {
        let Some(uri) = val(b, "cons") else { continue };
        let entry = match out.iter_mut().find(|c| c.uri == uri) {
            Some(e) => e,
            None => {
                out.push(Consultation {
                    uri: uri.to_string(),
                    status: val(b, "status").map(str::to_string),
                    start_date: None,
                    end_date: None,
                    institution: None,
                });
                out.last_mut().expect("just pushed")
            }
        };
        if entry.start_date.is_none() {
            entry.start_date = val(b, "start").map(str::to_string);
        }
        if entry.end_date.is_none() {
            entry.end_date = val(b, "end").map(str::to_string);
        }
        if entry.institution.is_none() {
            entry.institution = val(b, "inst").map(str::to_string);
        }
    }
    Ok(out)
}

/// Ein Vernehmlassungs-Dokument (Stellungnahme oder Ergebnisbericht).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsultationDocument {
    /// URI des Dokuments.
    pub uri: String,
    /// RDF-Klasse (`PositionStatementPublication` | `ResultOfAConsultationPublication`).
    pub kind: String,
}

const CONS_DOCS_Q: &str = r#"SELECT DISTINCT ?doc ?kind WHERE {
  {
    ?doc a jolux:PositionStatementPublication ;
         jolux:isOpinionOf ?x .
    BIND("PositionStatementPublication" AS ?kind)
    FILTER(STRSTARTS(STR(?x), "__CONS__"))
  } UNION {
    ?doc a jolux:ResultOfAConsultationPublication ;
         jolux:isOpinionOf ?x .
    BIND("ResultOfAConsultationPublication" AS ?kind)
    FILTER(STRSTARTS(STR(?x), "__CONS__"))
  }
} LIMIT 200"#;

/// JLX-GEN-03: Stellungnahmen und Ergebnisberichte einer Vernehmlassung.
///
/// Enumerate über die beiden Publikations-Klassen (J10.2: 873 + 1'789),
/// verknüpft via `isOpinionOf` auf die Consultation bzw. ihre Tasks.
pub async fn get_consultation_documents(
    client: &impl SparqlClient,
    consultation_uri: &str,
) -> Result<Vec<ConsultationDocument>, JoluxError> {
    let safe = consultation_uri.replace(['<', '>', '"', '\\', ' '], "");
    let sparql = format!("{PREFIXES}{}", CONS_DOCS_Q.replace("__CONS__", &safe));
    let res = client.query(&sparql).await?;
    Ok(res
        .bindings()
        .iter()
        .filter_map(|b| {
            Some(ConsultationDocument {
                uri: val(b, "doc")?.to_string(),
                kind: val(b, "kind")?.to_string(),
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
    async fn drafts_aggregate_resulting_resources() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["draft","draftId","parlId","result"]},"results":{"bindings":[
              {"draft":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/proj/2012/1295"},
               "draftId":{"type":"literal","value":"20121295"},
               "parlId":{"type":"literal","value":"13.074"},
               "result":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/oc/2017/762"}},
              {"draft":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/proj/2012/1295"},
               "draftId":{"type":"literal","value":"20121295"},
               "parlId":{"type":"literal","value":"13.074"},
               "result":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/fga/2016/1642"}}
            ]}}"#,
        );
        let eli = Eli::new("eli/cc/2017/762").unwrap();
        let resp = get_drafts(&client, &eli, ValidAsOf::new(date!(2026 - 01 - 01)))
            .await
            .unwrap();
        assert_eq!(resp.data().len(), 1, "ein Draft, nicht pro Binding einer");
        let d = &resp.data()[0];
        assert_eq!(d.parliament_draft_id.as_deref(), Some("13.074"));
        assert_eq!(d.resulting_resources.len(), 2);

        let q = client.last_query().unwrap();
        assert!(q.contains("jolux:hasResultingLegalResource"));
        assert!(q.contains("jolux:basicAct"));
    }

    #[tokio::test]
    async fn consultations_traverse_subtask_for_dates() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["cons","status","start","end","inst"]},"results":{"bindings":[
              {"cons":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/dl/proj/2021/100/cons_1"},
               "status":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/consultation-status/finished"},
               "start":{"type":"literal","value":"2021-05-01"},
               "end":{"type":"literal","value":"2021-08-31"}}
            ]}}"#,
        );
        let cons = get_consultations(&client, "https://fedlex.data.admin.ch/eli/dl/proj/2021/100")
            .await
            .unwrap();
        assert_eq!(cons.len(), 1);
        assert_eq!(cons[0].start_date.as_deref(), Some("2021-05-01"));

        let q = client.last_query().unwrap();
        assert!(
            q.contains("jolux:hasSubTask"),
            "Task-Zwischenknoten (J10.2) fehlt"
        );
        assert!(q.contains("jolux:draftHasTask"));
    }

    #[tokio::test]
    async fn consultation_documents_cover_both_kinds() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["doc","kind"]},"results":{"bindings":[
              {"doc":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/dl/proj/2021/100/cons_1/doc_21"},
               "kind":{"type":"literal","value":"PositionStatementPublication"}},
              {"doc":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/dl/proj/2021/100/cons_1/doc_31"},
               "kind":{"type":"literal","value":"ResultOfAConsultationPublication"}}
            ]}}"#,
        );
        let docs = get_consultation_documents(
            &client,
            "https://fedlex.data.admin.ch/eli/dl/proj/2021/100/cons_1",
        )
        .await
        .unwrap();
        assert_eq!(docs.len(), 2);

        let q = client.last_query().unwrap();
        assert!(q.contains("PositionStatementPublication"));
        assert!(q.contains("ResultOfAConsultationPublication"));
        assert!(q.contains("UNION"));
    }

    #[tokio::test]
    async fn consultation_uri_is_sanitized() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["doc","kind"]},"results":{"bindings":[]}}"#,
        );
        let _ = get_consultation_documents(&client, "https://x/> } INJECT {")
            .await
            .unwrap();
        let q = client.last_query().unwrap();
        assert!(!q.contains("> }"), "URI-Breakout nicht neutralisiert");
    }
}
