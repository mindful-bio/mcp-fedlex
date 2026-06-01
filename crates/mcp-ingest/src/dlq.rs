//! Dead-Letter-Queue für Poison-Releases (ADR-003, Entscheidung 2).
//!
//! Ein einziges fehlerhaftes Release (Schema-Verstoss, Parser-Panic) darf den
//! Ingestion-Strom nicht anhalten. Nach begrenzten Retries wandert die
//! Nachricht hierher, mit vollem Kontext für eine spätere manuelle
//! Re-Ingestion. Die DLQ blockiert nie nachfolgende valide Releases.

use std::sync::Mutex;

/// Ein in die DLQ verschobenes Release samt Diagnose-Kontext.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadLetter {
    /// Stabile Release-Kennung (für gezielte Re-Ingestion).
    pub release_id: String,
    /// Die unveränderte Rohnachricht, damit nach einem Fix neu eingespielt werden kann.
    pub raw: String,
    /// Menschenlesbare Fehlerursache des letzten Versuchs.
    pub reason: String,
    /// Anzahl der erfolglosen Verarbeitungsversuche bis zur Aufgabe.
    pub attempts: u32,
}

/// Thread-sichere Dead-Letter-Queue. Teilbar über `Arc`.
#[derive(Debug, Default)]
pub struct DeadLetterQueue {
    entries: Mutex<Vec<DeadLetter>>,
}

impl DeadLetterQueue {
    /// Erzeugt eine leere DLQ.
    pub fn new() -> Self {
        Self::default()
    }

    /// Verschiebt ein Release mit Kontext in die DLQ.
    pub fn push(&self, letter: DeadLetter) {
        self.entries
            .lock()
            .expect("dlq mutex poisoned")
            .push(letter);
    }

    /// Aktuelle Tiefe der DLQ (für Alarmierung und Metrik).
    pub fn depth(&self) -> usize {
        self.entries.lock().expect("dlq mutex poisoned").len()
    }

    /// Schnappschuss aller Einträge (für Inspektion und Tests).
    pub fn entries(&self) -> Vec<DeadLetter> {
        self.entries.lock().expect("dlq mutex poisoned").clone()
    }

    /// Entnimmt alle Einträge zur bewussten Re-Ingestion (Re-Drive).
    ///
    /// Leert die DLQ und gibt die Rohnachrichten zur erneuten Einspeisung
    /// zurück. Erst nach Behebung der Ursache aufzurufen.
    pub fn drain_for_redrive(&self) -> Vec<DeadLetter> {
        let mut guard = self.entries.lock().expect("dlq mutex poisoned");
        std::mem::take(&mut *guard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_carries_full_context() {
        let dlq = DeadLetterQueue::new();
        dlq.push(DeadLetter {
            release_id: "rel-1".into(),
            raw: "<broken/>".into(),
            reason: "schema violation".into(),
            attempts: 3,
        });
        assert_eq!(dlq.depth(), 1);
        let e = &dlq.entries()[0];
        assert_eq!(e.release_id, "rel-1");
        assert_eq!(e.attempts, 3);
        assert_eq!(e.raw, "<broken/>");
    }

    #[test]
    fn redrive_drains_and_empties() {
        let dlq = DeadLetterQueue::new();
        dlq.push(DeadLetter {
            release_id: "rel-1".into(),
            raw: "x".into(),
            reason: "boom".into(),
            attempts: 5,
        });
        let drained = dlq.drain_for_redrive();
        assert_eq!(drained.len(), 1);
        assert_eq!(dlq.depth(), 0);
    }
}
