//! Primitive: Vocabulary-Label-Auflösung (Rulebook J5).
//!
//! JOLux-Werte sind opake URIs (`.../resource-type/21`). Ohne SKOS-Lookup sind
//! sie bedeutungslos. Dies ist ein **reiner Helfer** ohne Provenance, da ein
//! Vokabular-Label keine Rechtsaussage ist.

use crate::client::{val, Language, SparqlClient, PREFIXES};
use crate::error::JoluxError;

const VOCAB_Q: &str = r#"SELECT ?label WHERE {
  <__VOCAB__> skos:prefLabel ?label .
  FILTER(LANG(?label) = "__TAG__")
} LIMIT 1"#;

/// Löst ein opakes JOLux-Vocabulary-URI zu seinem `skos:prefLabel` auf.
///
/// `.../resource-type/21` + `De` -> `"Bundesgesetz"`. Liefert
/// [`JoluxError::NotFound`], wenn es kein Label in der Sprache gibt — laut
/// Rulebook J5.4 fehlt bei 6 Katalogen das DE-Label, dann auf EN/FR/IT ausweichen.
pub async fn resolve_vocabulary_label(
    client: &impl SparqlClient,
    vocab_uri: &str,
    lang: Language,
) -> Result<String, JoluxError> {
    let sparql = format!(
        "{PREFIXES}{}",
        VOCAB_Q
            .replace("__VOCAB__", vocab_uri)
            .replace("__TAG__", lang.tag())
    );
    let res = client.query(&sparql).await?;
    res.bindings()
        .first()
        .and_then(|b| val(b, "label"))
        .map(str::to_string)
        .ok_or_else(|| JoluxError::NotFound(vocab_uri.to_string()))
}

/// Ein Konzept eines SKOS-Vokabulars.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VocabularyConcept {
    /// URI des Konzepts (`.../vocabulary/<scheme>/<code>`).
    pub uri: String,
    /// `skos:prefLabel` in der angefragten Sprache, sofern vorhanden.
    pub label: Option<String>,
}

/// Basis-URI der Fedlex-Vokabulare.
pub const VOCABULARY_BASE: &str = "https://fedlex.data.admin.ch/vocabulary/";

const LIST_VOCAB_Q: &str = r#"SELECT DISTINCT ?concept ?label WHERE {
  ?concept skos:prefLabel ?any .
  FILTER(STRSTARTS(STR(?concept), "__SCHEME__"))
  OPTIONAL { ?concept skos:prefLabel ?label . FILTER(LANG(?label) = "__TAG__") }
} LIMIT __LIMIT__"#;

/// JLX-VOC-02: Listet alle Konzepte eines SKOS-Schemes
/// (z.B. `resource-type`, `impact-type`, `enforcement-status`).
///
/// Achtung Definitions-/Nutzungs-Lücke (Rulebook J5.5): `enforcement-status`
/// definiert 6 Codes, genutzt werden 3. Bei 6 Katalogen fehlt das DE-Label
/// (J5.4) — `label` ist deshalb optional.
pub async fn list_vocabulary(
    client: &impl SparqlClient,
    scheme_id: &str,
    lang: Language,
    limit: u32,
) -> Result<Vec<VocabularyConcept>, JoluxError> {
    let safe = scheme_id.replace(['<', '>', '"', '\\', ' ', '/'], "");
    let sparql = format!(
        "{PREFIXES}{}",
        LIST_VOCAB_Q
            .replace("__SCHEME__", &format!("{VOCABULARY_BASE}{safe}/"))
            .replace("__TAG__", lang.tag())
            .replace("__LIMIT__", &limit.to_string())
    );
    let res = client.query(&sparql).await?;

    let mut out: Vec<VocabularyConcept> = Vec::new();
    for b in res.bindings() {
        let Some(uri) = val(b, "concept") else {
            continue;
        };
        match out.iter_mut().find(|c| c.uri == uri) {
            Some(existing) => {
                if existing.label.is_none() {
                    existing.label = val(b, "label").map(str::to_string);
                }
            }
            None => out.push(VocabularyConcept {
                uri: uri.to_string(),
                label: val(b, "label").map(str::to_string),
            }),
        }
    }
    Ok(out)
}

/// Eine Kante beim generischen Triple-Browsing.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NodeEdge {
    /// Prädikat-URI.
    pub predicate: String,
    /// Objekt (ausgehend) bzw. Subjekt (eingehend).
    pub value: String,
}

/// Nachbarschaft eines Knotens (ausgehende + eingehende Kanten).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NodeNeighborhood {
    /// Ausgehende Kanten (`<uri> ?p ?o`).
    pub outgoing: Vec<NodeEdge>,
    /// Eingehende Kanten (`?s ?p <uri>`).
    pub incoming: Vec<NodeEdge>,
}

const EXPLORE_OUT_Q: &str = r#"SELECT DISTINCT ?p ?o WHERE { <__URI__> ?p ?o } LIMIT __LIMIT__"#;
const EXPLORE_IN_Q: &str = r#"SELECT DISTINCT ?s ?p WHERE { ?s ?p <__URI__> } LIMIT __LIMIT__"#;

/// JLX-VOC-03: Generisches Triple-Browsing um einen beliebigen Knoten.
///
/// Escape-Hatch und Sicherheitsnetz für Ontologie-Drift (J0.3: ~12 ausgehende,
/// ~86 eingehende Kanten pro Gesetz). `DISTINCT` filtert die Quad-Store-
/// Duplikate (`rdf:type` bis 28× dupliziert, J16.2). Pagination über `limit`.
pub async fn explore_node(
    client: &impl SparqlClient,
    uri: &str,
    limit: u32,
) -> Result<NodeNeighborhood, JoluxError> {
    let safe = uri.replace(['<', '>', '"', '\\', ' '], "");
    let lim = limit.to_string();

    let out_q = format!(
        "{PREFIXES}{}",
        EXPLORE_OUT_Q
            .replace("__URI__", &safe)
            .replace("__LIMIT__", &lim)
    );
    let outgoing = client
        .query(&out_q)
        .await?
        .bindings()
        .iter()
        .filter_map(|b| {
            Some(NodeEdge {
                predicate: val(b, "p")?.to_string(),
                value: val(b, "o")?.to_string(),
            })
        })
        .collect();

    let in_q = format!(
        "{PREFIXES}{}",
        EXPLORE_IN_Q
            .replace("__URI__", &safe)
            .replace("__LIMIT__", &lim)
    );
    let incoming = client
        .query(&in_q)
        .await?
        .bindings()
        .iter()
        .filter_map(|b| {
            Some(NodeEdge {
                predicate: val(b, "p")?.to_string(),
                value: val(b, "s")?.to_string(),
            })
        })
        .collect();

    Ok(NodeNeighborhood { outgoing, incoming })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::MockSparqlClient;

    #[tokio::test]
    async fn resolves_german_label() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["label"]},"results":{"bindings":[
              {"label":{"type":"literal","xml:lang":"de","value":"Bundesgesetz"}}]}}"#,
        );
        let label = resolve_vocabulary_label(
            &client,
            "https://fedlex.data.admin.ch/vocabulary/resource-type/21",
            Language::De,
        )
        .await
        .unwrap();
        assert_eq!(label, "Bundesgesetz");

        let q = client.last_query().unwrap();
        assert!(q.contains("skos:prefLabel"));
        assert!(q.contains(r#"FILTER(LANG(?label) = "de")"#));
    }

    #[tokio::test]
    async fn missing_label_is_not_found() {
        let client =
            MockSparqlClient::from_json(r#"{"head":{"vars":["label"]},"results":{"bindings":[]}}"#);
        let err = resolve_vocabulary_label(&client, "https://example/vocab/x", Language::De)
            .await
            .unwrap_err();
        assert!(matches!(err, JoluxError::NotFound(_)));
    }

    #[tokio::test]
    async fn list_vocabulary_scopes_to_scheme_and_keeps_unlabeled() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["concept","label"]},"results":{"bindings":[
              {"concept":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/resource-type/21"},
               "label":{"type":"literal","xml:lang":"de","value":"Bundesgesetz"}},
              {"concept":{"type":"uri","value":"https://fedlex.data.admin.ch/vocabulary/resource-type/22"}}
            ]}}"#,
        );
        let concepts = list_vocabulary(&client, "resource-type", Language::De, 60)
            .await
            .unwrap();
        assert_eq!(concepts.len(), 2);
        assert_eq!(concepts[0].label.as_deref(), Some("Bundesgesetz"));
        assert!(
            concepts[1].label.is_none(),
            "fehlendes DE-Label bleibt None (J5.4)"
        );

        let q = client.last_query().unwrap();
        assert!(q.contains(
            r#"STRSTARTS(STR(?concept), "https://fedlex.data.admin.ch/vocabulary/resource-type/")"#
        ));
        assert!(q.contains("LIMIT 60"));
    }

    #[tokio::test]
    async fn explore_node_returns_both_directions() {
        let client = MockSparqlClient::from_json(
            r#"{"head":{"vars":["p","o","s"]},"results":{"bindings":[
              {"p":{"type":"uri","value":"http://data.legilux.public.lu/resource/ontology/jolux#basicAct"},
               "o":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/oc/2017/762"},
               "s":{"type":"uri","value":"https://fedlex.data.admin.ch/eli/cc/2017/762/20180101"}}
            ]}}"#,
        );
        let hood = explore_node(&client, "https://fedlex.data.admin.ch/eli/cc/2017/762", 50)
            .await
            .unwrap();
        assert_eq!(hood.outgoing.len(), 1);
        assert_eq!(hood.incoming.len(), 1);
        assert!(hood.outgoing[0].predicate.contains("basicAct"));

        let q = client.last_query().unwrap();
        assert!(
            q.contains("?p <https://fedlex.data.admin.ch/eli/cc/2017/762>"),
            "zweite Query = eingehend"
        );
        assert!(
            q.contains("SELECT DISTINCT"),
            "Quad-Store-Duplikate (J16.2) brauchen DISTINCT"
        );
    }
}
