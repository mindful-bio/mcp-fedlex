//! Bi-temporales Korpus im eingebetteten Oxigraph (ADR aus M2).
//!
//! Der Korpus ist append-only. Jede Norm-Fassung wird als eigene RDF-Ressource
//! geschrieben, nie überschrieben. Zwei Zeitachsen leben nebeneinander. Die
//! Gültigkeitszeit (`validFrom`) sagt, ab wann eine Fassung rechtlich galt. Die
//! Transaktionszeit (`transactionTime`) sagt, ab wann der Dienst sie kannte.
//!
//! Eine Punkt-in-Zeit-Abfrage liefert die zum Stichtag gültige Fassung mit dem
//! jüngsten Wissensstand. Eine spätere Korrektur überschreibt nichts, sie legt
//! eine neue Fassung mit höherer Transaktionszeit an. So bleibt der historische
//! Wissensstand rekonstruierbar, und stiller Drift ist ausgeschlossen.
//!
//! Oxigraph läuft eingebettet (in-process), die Tests brauchen kein Docker.

use std::sync::atomic::{AtomicU64, Ordering};

use oxigraph::model::{GraphName, Literal, NamedNode, Quad, Term};
use oxigraph::sparql::QueryResults;
use oxigraph::store::Store;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{Date, OffsetDateTime};

/// Namespace der Korpus-Prädikate.
const NS: &str = "https://fedlex.local/ns#";
/// Namespace der Fassungs-Ressourcen.
const VERSION_NS: &str = "https://fedlex.local/version/";
/// XSD-Datentyp für reine Datumsangaben.
const XSD_DATE: &str = "http://www.w3.org/2001/XMLSchema#date";
/// XSD-Datentyp für Zeitstempel.
const XSD_DATETIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";

/// Fehler des Korpus. Hält die Oxigraph-Typen aus der öffentlichen API heraus.
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    /// Der eingebettete Graph meldete einen Speicher-Fehler.
    #[error("graph storage error: {0}")]
    Storage(String),
    /// Eine SPARQL-Abfrage scheiterte.
    #[error("sparql evaluation error: {0}")]
    Query(String),
    /// Ein Datum/Zeitstempel liess sich nicht formatieren.
    #[error("temporal formatting error: {0}")]
    Format(String),
}

/// Append-only, bi-temporaler Korpus über einem eingebetteten Oxigraph.
pub struct OxigraphCorpus {
    store: Store,
    next_id: AtomicU64,
}

impl OxigraphCorpus {
    /// Erzeugt einen leeren In-Memory-Korpus.
    pub fn new() -> Result<Self, GraphError> {
        let store = Store::new().map_err(|e| GraphError::Storage(e.to_string()))?;
        Ok(Self {
            store,
            next_id: AtomicU64::new(0),
        })
    }

    /// Hängt eine Norm-Fassung an. Append-only, nie destruktiv.
    ///
    /// `valid_from` ist der Beginn der Gültigkeit, `transaction_time` der
    /// Zeitpunkt, zu dem der Dienst die Fassung kennt. Jeder Aufruf legt eine
    /// frische Ressource an, auch bei gleicher `version_id` (etwa eine spätere
    /// Korrektur mit höherer Transaktionszeit).
    pub fn append_version(
        &self,
        eli: &str,
        version_id: &str,
        valid_from: Date,
        transaction_time: OffsetDateTime,
        text: &str,
    ) -> Result<(), GraphError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let subject = self.version_node(id)?;

        let vf = valid_from
            .format(format_description!("[year]-[month]-[day]"))
            .map_err(|e| GraphError::Format(e.to_string()))?;
        let tt = transaction_time
            .format(&Rfc3339)
            .map_err(|e| GraphError::Format(e.to_string()))?;

        let date_type = self.named(XSD_DATE)?;
        let datetime_type = self.named(XSD_DATETIME)?;

        let quads = [
            Quad::new(
                subject.clone(),
                self.named(&format!("{NS}eli"))?,
                Literal::new_simple_literal(eli),
                GraphName::DefaultGraph,
            ),
            Quad::new(
                subject.clone(),
                self.named(&format!("{NS}versionId"))?,
                Literal::new_simple_literal(version_id),
                GraphName::DefaultGraph,
            ),
            Quad::new(
                subject.clone(),
                self.named(&format!("{NS}validFrom"))?,
                Literal::new_typed_literal(vf, date_type),
                GraphName::DefaultGraph,
            ),
            Quad::new(
                subject.clone(),
                self.named(&format!("{NS}transactionTime"))?,
                Literal::new_typed_literal(tt, datetime_type),
                GraphName::DefaultGraph,
            ),
            Quad::new(
                subject,
                self.named(&format!("{NS}text"))?,
                Literal::new_simple_literal(text),
                GraphName::DefaultGraph,
            ),
        ];

        for quad in quads {
            self.store
                .insert(&quad)
                .map_err(|e| GraphError::Storage(e.to_string()))?;
        }
        Ok(())
    }

    /// Ob eine Fassung mit diesem ELI und dieser Fassungs-Kennung existiert.
    ///
    /// Dient der Idempotenz-Prüfung des Writers (ADR-003).
    pub fn contains_version(&self, eli: &str, version_id: &str) -> Result<bool, GraphError> {
        let query = format!(
            "PREFIX fx: <{NS}>
             ASK {{ ?v fx:eli {eli} ; fx:versionId {ver} }}",
            eli = sparql_string(eli),
            ver = sparql_string(version_id),
        );
        match self.query(&query)? {
            QueryResults::Boolean(b) => Ok(b),
            _ => Ok(false),
        }
    }

    /// Löst die zum Stichtag gültige Fassung mit dem jüngsten Wissensstand auf.
    ///
    /// Bi-temporal korrekt. Unter allen Fassungen mit `validFrom <= as_of` wird
    /// die mit dem grössten `validFrom` gewählt, und unter diesen die mit der
    /// höchsten `transactionTime`. So gewinnt eine spätere Korrektur, ohne dass
    /// die ursprüngliche Fassung verloren geht.
    pub fn resolve_as_of(&self, eli: &str, as_of: Date) -> Result<Option<String>, GraphError> {
        let as_of_str = as_of
            .format(format_description!("[year]-[month]-[day]"))
            .map_err(|e| GraphError::Format(e.to_string()))?;
        let query = format!(
            "PREFIX fx: <{NS}>
             PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
             SELECT ?text WHERE {{
                 ?v fx:eli {eli} ;
                    fx:validFrom ?vf ;
                    fx:transactionTime ?tt ;
                    fx:text ?text .
                 FILTER(?vf <= {as_of}^^xsd:date)
             }}
             ORDER BY DESC(?vf) DESC(?tt)
             LIMIT 1",
            eli = sparql_string(eli),
            as_of = sparql_string(&as_of_str),
        );

        match self.query(&query)? {
            QueryResults::Solutions(mut solutions) => match solutions.next() {
                Some(row) => {
                    let row = row.map_err(|e| GraphError::Query(e.to_string()))?;
                    Ok(row.get("text").and_then(literal_value))
                }
                None => Ok(None),
            },
            _ => Ok(None),
        }
    }

    /// Anzahl der für einen ELI gespeicherten Fassungen.
    ///
    /// Macht die Append-only-Eigenschaft prüfbar (eine Korrektur erhöht die
    /// Zahl, statt eine Fassung zu ersetzen).
    pub fn version_count(&self, eli: &str) -> Result<usize, GraphError> {
        let query = format!(
            "PREFIX fx: <{NS}>
             SELECT (COUNT(?v) AS ?c) WHERE {{ ?v fx:eli {eli} }}",
            eli = sparql_string(eli),
        );
        match self.query(&query)? {
            QueryResults::Solutions(mut solutions) => match solutions.next() {
                Some(row) => {
                    let row = row.map_err(|e| GraphError::Query(e.to_string()))?;
                    Ok(row
                        .get("c")
                        .and_then(literal_value)
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0))
                }
                None => Ok(0),
            },
            _ => Ok(0),
        }
    }

    fn version_node(&self, id: u64) -> Result<NamedNode, GraphError> {
        self.named(&format!("{VERSION_NS}{id}"))
    }

    fn named(&self, iri: &str) -> Result<NamedNode, GraphError> {
        NamedNode::new(iri).map_err(|e| GraphError::Storage(e.to_string()))
    }

    fn query(&self, query: &str) -> Result<QueryResults, GraphError> {
        self.store
            .query(query)
            .map_err(|e| GraphError::Query(e.to_string()))
    }
}

/// Liest den Lexical-Wert aus einem Literal-Term, sonst `None`.
fn literal_value(term: &Term) -> Option<String> {
    match term {
        Term::Literal(lit) => Some(lit.value().to_string()),
        _ => None,
    }
}

/// Schreibt einen String als sicheres SPARQL-Literal und entschärft Anführungs-
/// und Backslash-Zeichen. Verhindert SPARQL-Injection über ELI oder Datum.
fn sparql_string(raw: &str) -> String {
    let escaped = raw
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::{date, datetime};

    #[test]
    fn append_then_resolve_returns_text_valid_at_date() {
        let corpus = OxigraphCorpus::new().unwrap();
        corpus
            .append_version(
                "eli/cc/1999/404",
                "1999-01-01",
                date!(1999 - 01 - 01),
                datetime!(1999-01-01 00:00 UTC),
                "Fassung A",
            )
            .unwrap();

        let hit = corpus
            .resolve_as_of("eli/cc/1999/404", date!(2000 - 06 - 01))
            .unwrap();
        assert_eq!(hit.as_deref(), Some("Fassung A"));
    }

    #[test]
    fn resolve_before_first_validity_is_none() {
        let corpus = OxigraphCorpus::new().unwrap();
        corpus
            .append_version(
                "eli/cc/1999/404",
                "2010-01-01",
                date!(2010 - 01 - 01),
                datetime!(2010-01-01 00:00 UTC),
                "Erst ab 2010",
            )
            .unwrap();

        let hit = corpus
            .resolve_as_of("eli/cc/1999/404", date!(2005 - 01 - 01))
            .unwrap();
        assert_eq!(hit, None);
    }

    #[test]
    fn picks_latest_validity_not_after_stichtag() {
        let corpus = OxigraphCorpus::new().unwrap();
        corpus
            .append_version(
                "eli/cc/1999/404",
                "2000",
                date!(2000 - 01 - 01),
                datetime!(2000-01-01 00:00 UTC),
                "Stand 2000",
            )
            .unwrap();
        corpus
            .append_version(
                "eli/cc/1999/404",
                "2015",
                date!(2015 - 01 - 01),
                datetime!(2015-01-01 00:00 UTC),
                "Stand 2015",
            )
            .unwrap();

        // Stichtag 2010 sieht die Fassung von 2000, nicht die spätere von 2015.
        let hit = corpus
            .resolve_as_of("eli/cc/1999/404", date!(2010 - 01 - 01))
            .unwrap();
        assert_eq!(hit.as_deref(), Some("Stand 2000"));
    }

    #[test]
    fn correction_wins_by_transaction_time_without_losing_history() {
        let corpus = OxigraphCorpus::new().unwrap();
        // Ursprüngliche Erfassung der ab 2000 gültigen Fassung.
        corpus
            .append_version(
                "eli/cc/1999/404",
                "2000",
                date!(2000 - 01 - 01),
                datetime!(2001-03-01 09:00 UTC),
                "Erstfassung mit Tippfehler",
            )
            .unwrap();
        // Spätere Korrektur derselben Gültigkeitszeit, höhere Transaktionszeit.
        corpus
            .append_version(
                "eli/cc/1999/404",
                "2000",
                date!(2000 - 01 - 01),
                datetime!(2002-06-01 09:00 UTC),
                "Korrigierte Fassung",
            )
            .unwrap();

        // Die Abfrage liefert den jüngsten Wissensstand.
        let hit = corpus
            .resolve_as_of("eli/cc/1999/404", date!(2005 - 01 - 01))
            .unwrap();
        assert_eq!(hit.as_deref(), Some("Korrigierte Fassung"));

        // Die alte Fassung lebt weiter (append-only, kein Verlust).
        assert_eq!(corpus.version_count("eli/cc/1999/404").unwrap(), 2);
    }

    #[test]
    fn contains_version_is_true_only_after_write() {
        let corpus = OxigraphCorpus::new().unwrap();
        assert!(!corpus.contains_version("eli/cc/1999/404", "2000").unwrap());
        corpus
            .append_version(
                "eli/cc/1999/404",
                "2000",
                date!(2000 - 01 - 01),
                datetime!(2000-01-01 00:00 UTC),
                "Fassung",
            )
            .unwrap();
        assert!(corpus.contains_version("eli/cc/1999/404", "2000").unwrap());
    }

    #[test]
    fn injection_attempt_in_eli_does_not_break_query() {
        let corpus = OxigraphCorpus::new().unwrap();
        // Ein ELI mit Anführungszeichen darf die Abfrage nicht zerbrechen.
        let nasty = "eli\" } ASK { ?x ?y ?z } #";
        let hit = corpus.resolve_as_of(nasty, date!(2020 - 01 - 01)).unwrap();
        assert_eq!(hit, None);
        assert_eq!(corpus.version_count(nasty).unwrap(), 0);
    }
}
