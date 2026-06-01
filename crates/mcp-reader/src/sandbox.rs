//! Execution Sandbox & Timeout Guard (ADR-nah, Agent-Defense).
//!
//! Halluzinierte oder bösartige Such-Queries sind ein DoS-Vektor. Zwei
//! Schutzschichten.
//! 1. Linearzeit-Regex. Die `regex`-Crate kennt kein katastrophales
//!    Backtracking, zusätzlich begrenzt `size_limit` die kompilierte Grösse.
//!    Ein pathologisches Muster scheitert damit schon beim Kompilieren.
//! 2. Harte Wall-Clock-Deadline. Die Arbeit läuft via `spawn_blocking` auf
//!    einem separaten Thread, NICHT im async-Reactor. Nur so greift ein
//!    `tokio::time::timeout`. Liefe die Schleife im Reactor, würde ein
//!    blockierender Query den ganzen Pod einfrieren und kein Timeout zöge.
//!
//! Bei Abbruch entsteht ein lenkender Fehler, der von der Graceful-Failure-
//! Middleware ans LLM gereicht wird ("Suchanfrage zu komplex, bitte praeziser").

use std::time::Duration;

/// Fehler der Sandbox. Alle Varianten sind lenkend, nie ein Crash.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SandboxError {
    /// Das Muster war zu komplex und überschritt das `size_limit` beim Kompilieren.
    #[error("query too complex to compile: {0}")]
    TooComplex(String),
    /// Die Ausführung überschritt die Wall-Clock-Deadline.
    #[error("query exceeded deadline")]
    Deadline,
    /// Die Arbeit auf dem Blocking-Thread brach unerwartet ab (Panic/Join).
    #[error("sandbox worker failed")]
    WorkerFailed,
}

impl SandboxError {
    /// Lenkender Hinweis für das LLM.
    pub fn hint(&self) -> &'static str {
        match self {
            SandboxError::TooComplex(_) | SandboxError::Deadline => {
                "Suchanfrage zu komplex, bitte praeziser formulieren."
            }
            SandboxError::WorkerFailed => "Interner Fehler bei der Suche, bitte erneut versuchen.",
        }
    }
}

/// Sandbox mit harter Deadline und begrenzter Regex-Komplexität.
#[derive(Debug, Clone, Copy)]
pub struct SearchSandbox {
    deadline: Duration,
    regex_size_limit: usize,
}

impl Default for SearchSandbox {
    fn default() -> Self {
        // Konservative Default-Politik. ~2s Wall-Clock, 1 MiB Regex-Programm.
        Self {
            deadline: Duration::from_secs(2),
            regex_size_limit: 1 << 20,
        }
    }
}

impl SearchSandbox {
    /// Erzeugt eine Sandbox mit expliziter Deadline und Regex-Grössenlimit.
    pub fn new(deadline: Duration, regex_size_limit: usize) -> Self {
        Self {
            deadline,
            regex_size_limit,
        }
    }

    /// Führt eine beliebige CPU-gebundene Arbeit unter der Deadline aus.
    ///
    /// Die Arbeit läuft auf einem Blocking-Thread. Überschreitet sie die
    /// Deadline, kehrt der Aufruf mit [`SandboxError::Deadline`] zurück. Der
    /// Thread selbst kann nicht hart abgebrochen werden, deshalb MUSS jede hier
    /// laufende Arbeit von Natur aus terminieren (Linearzeit-Regex, begrenzte
    /// Eingaben). Die Deadline schützt die Antwortlatenz, die Linearzeit-
    /// Garantie schützt den Worker-Thread.
    pub async fn run<T, F>(&self, work: F) -> Result<T, SandboxError>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        let handle = tokio::task::spawn_blocking(work);
        match tokio::time::timeout(self.deadline, handle).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(_join_err)) => Err(SandboxError::WorkerFailed),
            Err(_elapsed) => Err(SandboxError::Deadline),
        }
    }

    /// Sucht alle Treffer eines LLM-gelieferten Musters im Heuhaufen.
    ///
    /// Das Muster wird mit begrenztem `size_limit` kompiliert (DoS-Schutz schon
    /// zur Compile-Zeit), die Suche läuft Linearzeit unter der Deadline.
    pub async fn find_all(
        &self,
        pattern: &str,
        haystack: String,
    ) -> Result<Vec<String>, SandboxError> {
        let limit = self.regex_size_limit;
        let pattern = pattern.to_string();
        self.run(move || {
            let re = regex::RegexBuilder::new(&pattern)
                .size_limit(limit)
                .build()
                .map_err(|e| SandboxError::TooComplex(e.to_string()))?;
            Ok::<_, SandboxError>(
                re.find_iter(&haystack)
                    .map(|m| m.as_str().to_string())
                    .collect(),
            )
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn finds_matches_with_linear_regex() {
        let sb = SearchSandbox::default();
        let hits = sb
            .find_all(r"Art\.\s*\d+", "Art. 1 und Art. 42 BV".to_string())
            .await
            .unwrap();
        assert_eq!(hits, vec!["Art. 1", "Art. 42"]);
    }

    #[tokio::test]
    async fn rejects_pattern_exceeding_size_limit() {
        // Winziges size_limit. Selbst ein moderates Muster scheitert schon
        // beim Kompilieren, lange bevor es Rechenzeit kostet.
        let sb = SearchSandbox::new(Duration::from_secs(2), 16);
        let err = sb
            .find_all(r"(a|b|c|d|e|f|g){10,100}", "aaa".to_string())
            .await
            .unwrap_err();
        assert!(matches!(err, SandboxError::TooComplex(_)));
        assert!(err.hint().contains("praeziser"));
    }

    #[tokio::test]
    async fn aborts_work_exceeding_deadline() {
        // Sehr kurze Deadline gegen eine bewusst langsame Arbeit.
        let sb = SearchSandbox::new(Duration::from_millis(50), 1 << 20);
        let err = sb
            .run(|| {
                std::thread::sleep(Duration::from_millis(500));
                42
            })
            .await
            .unwrap_err();
        assert_eq!(err, SandboxError::Deadline);
        assert!(err.hint().contains("praeziser"));
    }

    #[tokio::test]
    async fn fast_work_under_deadline_succeeds() {
        let sb = SearchSandbox::new(Duration::from_millis(500), 1 << 20);
        let out = sb.run(|| 2 + 2).await.unwrap();
        assert_eq!(out, 4);
    }
}
