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
//!
//! **Schema-Versionierung (B-2/D-2).** Der Korpus trägt eine [`SCHEMA_VERSION`].
//! Sie wandert in jeden Backup-Kopf. Beim Restore wird sie geprüft: eine fremde
//! Version bricht hart mit [`GraphError::SchemaMismatch`] ab, statt still
//! inkompatible Daten zu laden. Wird das RDF-Schema (Prädikate/Form) je
//! verändert, ist die Konstante zu erhöhen und eine Migrationsnotiz an
//! [`restore_from_str`](OxigraphCorpus::restore_from_str) zu ergänzen.
//!
//! **Backup/Restore (B-1/D-1).** [`dump_to_string`](OxigraphCorpus::dump_to_string)
//! schreibt einen vollständigen, menschenlesbaren Schnappschuss (Kopf + eine
//! Zeile je Fassung). [`restore_from_str`](OxigraphCorpus::restore_from_str) baut
//! daraus deterministisch einen frischen Korpus. Der Roundtrip ist append-only-
//! treu: Gültigkeits- und Transaktionszeit jeder Fassung bleiben erhalten, die
//! Historie (mehrere Fassungen pro ELI) geht nicht verloren.

use std::sync::atomic::{AtomicU64, Ordering};

use oxigraph::model::{GraphName, Literal, NamedNode, Quad, Term};
use oxigraph::sparql::QueryResults;
use oxigraph::store::Store;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{Date, OffsetDateTime};

/// Schema-Version des bi-temporalen Korpus-RDF (Prädikate + Fassungs-Form).
///
/// Wird in jeden Backup-Kopf geschrieben und beim Restore geprüft. **Erhöhen,
/// sobald sich Prädikat-Namen, Datentypen oder die Fassungs-Struktur ändern**,
/// und dann die Migrationsnotiz in [`OxigraphCorpus::restore_from_str`] ergänzen.
pub const SCHEMA_VERSION: u32 = 1;

/// Kopfzeilen-Präfix eines Korpus-Backups. Trägt die Schema-Version.
const BACKUP_HEADER_PREFIX: &str = "#fedlex-corpus-backup v";

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
    /// Ein Backup hatte ein nicht lesbares oder beschädigtes Format.
    #[error("corrupt backup: {0}")]
    CorruptBackup(String),
    /// Ein Restore wurde mit einer fremden Schema-Version versucht (D-2-Guard).
    #[error("schema version mismatch: backup is v{found}, this build expects v{expected}")]
    SchemaMismatch {
        /// Die im Backup-Kopf gefundene Version.
        found: u32,
        /// Die von diesem Build erwartete [`SCHEMA_VERSION`].
        expected: u32,
    },
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

    /// Serialisiert den gesamten Korpus in einen versionierten Schnappschuss
    /// (Backup, D-1). Format: eine Kopfzeile `#fedlex-corpus-backup v<N>`,
    /// danach je Fassung eine tab-separierte Zeile
    /// `eli \t versionId \t validFrom \t transactionTime \t text`. Felder werden
    /// escaped, damit Tabs/Zeilenumbrüche im Text die Zeilenstruktur nicht
    /// sprengen. Die Reihenfolge ist deterministisch (nach ELI, dann Zeiten),
    /// damit zwei Dumps desselben Standes byte-gleich sind.
    pub fn dump_to_string(&self) -> Result<String, GraphError> {
        let query = format!(
            "PREFIX fx: <{NS}>
             SELECT ?eli ?ver ?vf ?tt ?text WHERE {{
                 ?v fx:eli ?eli ;
                    fx:versionId ?ver ;
                    fx:validFrom ?vf ;
                    fx:transactionTime ?tt ;
                    fx:text ?text .
             }}
             ORDER BY ?eli ?vf ?tt ?ver"
        );

        let mut out = format!("{BACKUP_HEADER_PREFIX}{SCHEMA_VERSION}\n");
        if let QueryResults::Solutions(solutions) = self.query(&query)? {
            for row in solutions {
                let row = row.map_err(|e| GraphError::Query(e.to_string()))?;
                let field = |name: &str| -> Result<String, GraphError> {
                    row.get(name)
                        .and_then(literal_value)
                        .ok_or_else(|| GraphError::CorruptBackup(format!("missing field {name}")))
                };
                let line = [
                    field("eli")?,
                    field("ver")?,
                    field("vf")?,
                    field("tt")?,
                    field("text")?,
                ]
                .iter()
                .map(|f| backup_escape(f))
                .collect::<Vec<_>>()
                .join("\t");
                out.push_str(&line);
                out.push('\n');
            }
        }
        Ok(out)
    }

    /// Baut aus einem Schnappschuss von [`dump_to_string`] einen frischen Korpus
    /// (Restore, D-1). Die Schema-Version im Kopf wird gegen [`SCHEMA_VERSION`]
    /// geprüft; eine Abweichung bricht hart mit [`GraphError::SchemaMismatch`]
    /// ab, statt inkompatible Daten still zu laden (D-2-Guard).
    ///
    /// **Migrationsnotiz.** Solange [`SCHEMA_VERSION`] `1` ist, gibt es nur ein
    /// Format und keine Migration. Wird die Version je erhöht, ist hier ein
    /// Upgrade-Pfad `v(N-1) -> vN` einzuziehen (Felder ergänzen/umbenennen),
    /// bevor der harte Abbruch greift — Altbackups bleiben so lesbar.
    pub fn restore_from_str(snapshot: &str) -> Result<Self, GraphError> {
        let mut lines = snapshot.lines();
        let header = lines
            .next()
            .ok_or_else(|| GraphError::CorruptBackup("empty snapshot".into()))?;
        let version = parse_backup_version(header)?;
        if version != SCHEMA_VERSION {
            return Err(GraphError::SchemaMismatch {
                found: version,
                expected: SCHEMA_VERSION,
            });
        }

        let corpus = Self::new()?;
        for (idx, raw) in lines.enumerate() {
            if raw.is_empty() {
                continue;
            }
            let cols: Vec<&str> = raw.split('\t').collect();
            if cols.len() != 5 {
                return Err(GraphError::CorruptBackup(format!(
                    "line {} has {} fields, expected 5",
                    idx + 2,
                    cols.len()
                )));
            }
            let eli = backup_unescape(cols[0]);
            let version_id = backup_unescape(cols[1]);
            let vf = backup_unescape(cols[2]);
            let tt = backup_unescape(cols[3]);
            let text = backup_unescape(cols[4]);

            let valid_from = Date::parse(&vf, format_description!("[year]-[month]-[day]"))
                .map_err(|e| GraphError::CorruptBackup(format!("bad validFrom '{vf}': {e}")))?;
            let transaction_time = OffsetDateTime::parse(&tt, &Rfc3339).map_err(|e| {
                GraphError::CorruptBackup(format!("bad transactionTime '{tt}': {e}"))
            })?;

            corpus.append_version(&eli, &version_id, valid_from, transaction_time, &text)?;
        }
        Ok(corpus)
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

/// Escaped ein Backup-Feld, damit Backslash, Tab und Zeilenumbruch die
/// tab-separierte Zeilenstruktur des Dumps nicht zerbrechen. Umkehrung:
/// [`backup_unescape`].
fn backup_escape(raw: &str) -> String {
    raw.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// Macht [`backup_escape`] rückgängig. Unbekannte Escapes bleiben unverändert.
fn backup_unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('\\') => out.push('\\'),
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Liest die Schema-Version aus der Backup-Kopfzeile `#fedlex-corpus-backup v<N>`.
fn parse_backup_version(header: &str) -> Result<u32, GraphError> {
    let rest = header.strip_prefix(BACKUP_HEADER_PREFIX).ok_or_else(|| {
        GraphError::CorruptBackup(format!("missing backup header, got: {header:?}"))
    })?;
    rest.trim()
        .parse::<u32>()
        .map_err(|e| GraphError::CorruptBackup(format!("bad version in header '{header}': {e}")))
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

    /// Befüllt einen Korpus mit zwei ELIs, davon einer mit Korrektur-Historie.
    fn seeded_corpus() -> OxigraphCorpus {
        let corpus = OxigraphCorpus::new().unwrap();
        corpus
            .append_version(
                "eli/cc/1999/404",
                "2000",
                date!(2000 - 01 - 01),
                datetime!(2001-03-01 09:00 UTC),
                "Erstfassung mit Tippfehler",
            )
            .unwrap();
        corpus
            .append_version(
                "eli/cc/1999/404",
                "2000",
                date!(2000 - 01 - 01),
                datetime!(2002-06-01 09:00 UTC),
                "Korrigierte Fassung",
            )
            .unwrap();
        corpus
            .append_version(
                "eli/cc/2010/77",
                "2010",
                date!(2010 - 01 - 01),
                datetime!(2010-01-01 00:00 UTC),
                "Andere Norm",
            )
            .unwrap();
        corpus
    }

    #[test]
    fn backup_header_carries_schema_version() {
        let dump = OxigraphCorpus::new().unwrap().dump_to_string().unwrap();
        assert!(dump.starts_with(&format!("{BACKUP_HEADER_PREFIX}{SCHEMA_VERSION}\n")));
    }

    #[test]
    fn dump_is_deterministic() {
        let a = seeded_corpus().dump_to_string().unwrap();
        let b = seeded_corpus().dump_to_string().unwrap();
        assert_eq!(a, b, "zwei Dumps desselben Standes müssen byte-gleich sein");
    }

    #[test]
    fn restore_roundtrip_preserves_resolution_and_history() {
        let original = seeded_corpus();
        let dump = original.dump_to_string().unwrap();

        let restored = OxigraphCorpus::restore_from_str(&dump).unwrap();

        // Bi-temporale Auflösung überlebt den Roundtrip (Korrektur gewinnt).
        assert_eq!(
            restored
                .resolve_as_of("eli/cc/1999/404", date!(2005 - 01 - 01))
                .unwrap()
                .as_deref(),
            Some("Korrigierte Fassung"),
        );
        // Append-only-Historie bleibt vollständig (kein Verlust der Erstfassung).
        assert_eq!(restored.version_count("eli/cc/1999/404").unwrap(), 2);
        assert_eq!(restored.version_count("eli/cc/2010/77").unwrap(), 1);

        // Ein erneuter Dump des Restores ist byte-gleich zum Original.
        assert_eq!(restored.dump_to_string().unwrap(), dump);
    }

    #[test]
    fn restore_roundtrip_survives_tabs_and_newlines_in_text() {
        let corpus = OxigraphCorpus::new().unwrap();
        let tricky = "Art. 1\tAbs. 2\nmit Zeilenumbruch und \\ Backslash";
        corpus
            .append_version(
                "eli/cc/2020/1",
                "2020",
                date!(2020 - 01 - 01),
                datetime!(2020-01-01 00:00 UTC),
                tricky,
            )
            .unwrap();

        let dump = corpus.dump_to_string().unwrap();
        // Genau eine Datenzeile trotz eingebettetem Zeilenumbruch.
        assert_eq!(dump.lines().count(), 2);

        let restored = OxigraphCorpus::restore_from_str(&dump).unwrap();
        assert_eq!(
            restored
                .resolve_as_of("eli/cc/2020/1", date!(2020 - 06 - 01))
                .unwrap()
                .as_deref(),
            Some(tricky),
        );
    }

    #[test]
    fn restore_rejects_foreign_schema_version() {
        // Ein Backup-Kopf einer fremden (zukünftigen) Schema-Version.
        let future = SCHEMA_VERSION + 1;
        let snapshot = format!("{BACKUP_HEADER_PREFIX}{future}\n");
        // `OxigraphCorpus` ist nicht `Debug`, daher kein `unwrap_err()`: direkt matchen.
        match OxigraphCorpus::restore_from_str(&snapshot) {
            Err(GraphError::SchemaMismatch { found, expected }) => {
                assert_eq!(found, future);
                assert_eq!(expected, SCHEMA_VERSION);
            }
            Err(other) => panic!("expected SchemaMismatch, got {other:?}"),
            Ok(_) => panic!("expected SchemaMismatch, got Ok"),
        }
    }

    #[test]
    fn restore_rejects_missing_header() {
        assert!(matches!(
            OxigraphCorpus::restore_from_str("eli\tver\t2000-01-01\tx\ttext\n"),
            Err(GraphError::CorruptBackup(_))
        ));
    }

    #[test]
    fn restore_rejects_truncated_line() {
        let snapshot =
            format!("{BACKUP_HEADER_PREFIX}{SCHEMA_VERSION}\neli\tver\tnur-drei-felder\n");
        assert!(matches!(
            OxigraphCorpus::restore_from_str(&snapshot),
            Err(GraphError::CorruptBackup(_))
        ));
    }

    #[test]
    fn empty_corpus_restore_roundtrips() {
        let dump = OxigraphCorpus::new().unwrap().dump_to_string().unwrap();
        let restored = OxigraphCorpus::restore_from_str(&dump).unwrap();
        assert_eq!(restored.version_count("egal").unwrap(), 0);
    }
}
