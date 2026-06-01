//! Herkunfts-Hülle (Provenance) nach ADR-004.
//!
//! Jede Tool-Antwort muss strukturell ihre Herkunft tragen, damit der Reasoner
//! (syllogismus-fedlex) jede Aussage auf eine exakte Norm-Version zurückführen
//! kann. `Provenance` bündelt die drei Pflichtfelder ELI, Gültigkeitszeit und
//! Systemzeit. Die Felder sind nicht-optional, ein fehlendes ELI ist ein Fehler
//! und kein Sonderfall.

use crate::eli::Eli;
use crate::temporal::{TransactionTime, ValidAsOf};
use serde::{Deserialize, Serialize};

/// Strukturelle Herkunft einer gelieferten Aussage (ADR-004).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// ELI der Quelle, auf die die Aussage zurückgeht.
    pub eli: Eli,
    /// Gültigkeitszeit. Welche Fassung galt am Stichtag.
    pub valid_as_of: ValidAsOf,
    /// Systemzeit. Wann die zugrunde liegende Information erfasst wurde.
    pub transaction_time: TransactionTime,
}

impl Provenance {
    /// Erzeugt eine vollständige Herkunfts-Hülle. Alle drei Felder sind Pflicht.
    pub fn new(eli: Eli, valid_as_of: ValidAsOf, transaction_time: TransactionTime) -> Self {
        Self {
            eli,
            valid_as_of,
            transaction_time,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::{date, datetime};

    fn sample() -> Provenance {
        Provenance::new(
            Eli::new("eli/cc/1999/404").unwrap(),
            ValidAsOf::new(date!(2020 - 01 - 01)),
            TransactionTime::new(datetime!(2026-06-01 09:00 UTC)),
        )
    }

    #[test]
    fn provenance_carries_all_three_fields() {
        let p = sample();
        assert_eq!(p.eli.as_str(), "eli/cc/1999/404");
        assert_eq!(p.valid_as_of.to_string(), "2020-01-01");
    }

    #[test]
    fn provenance_roundtrips_through_serde() {
        let p = sample();
        let json = serde_json::to_string(&p).unwrap();
        let back: Provenance = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }
}
