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
}
