//! Temporal Resolver (Point-in-Time, ADR-004-nah).
//!
//! Juristisch zwingend. Jede Abfrage bezieht sich auf einen Stichtag, nicht
//! blind auf die neueste Fassung. Der Resolver stempelt eine Anfrage mit
//! [`ValidAsOf`] (welche Fassung galt) und [`TransactionTime`] (wann erfasst).
//! Aus diesem Stempel und der aufgelösten Quelle entsteht später die
//! [`Provenance`] der Antwort. Damit hängen Anfrage-Stempel und Antwort-Herkunft
//! an denselben zwei Zeitachsen.

use fedlex_core::{Eli, Provenance, TransactionTime, ValidAsOf};
use time::Date;

/// Stempel einer einzelnen Anfrage. Bindet beide Zeitachsen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryStamp {
    valid_as_of: ValidAsOf,
    transaction_time: TransactionTime,
}

impl QueryStamp {
    /// Gültigkeitszeit der Anfrage.
    pub fn valid_as_of(&self) -> ValidAsOf {
        self.valid_as_of
    }

    /// Systemzeit der Anfrage.
    pub fn transaction_time(&self) -> TransactionTime {
        self.transaction_time
    }

    /// Leitet aus diesem Stempel und der aufgelösten Quelle die Antwort-Herkunft
    /// ab. So trägt jede Antwort exakt den Stichtag, gegen den gefragt wurde.
    pub fn into_provenance(self, eli: Eli) -> Provenance {
        Provenance::new(eli, self.valid_as_of, self.transaction_time)
    }
}

/// Stempelt Anfragen mit dem juristischen Stichtag.
#[derive(Debug, Clone, Copy)]
pub struct TemporalResolver {
    /// Stichtag, falls der Agent keinen angibt (Default. heutiges Datum).
    default_as_of: Date,
}

impl TemporalResolver {
    /// Erzeugt einen Resolver mit explizitem Default-Stichtag.
    pub fn new(default_as_of: Date) -> Self {
        Self { default_as_of }
    }

    /// Stempelt eine Anfrage. Gibt der Agent einen Stichtag an, gilt dieser,
    /// sonst der Default. Die Systemzeit ist immer der reale Erfassungszeitpunkt.
    pub fn stamp(&self, requested_as_of: Option<Date>) -> QueryStamp {
        let valid_as_of = ValidAsOf::new(requested_as_of.unwrap_or(self.default_as_of));
        QueryStamp {
            valid_as_of,
            transaction_time: TransactionTime::now(),
        }
    }

    /// Wie [`Self::stamp`], aber mit fester Systemzeit (für deterministische Tests).
    pub fn stamp_at(&self, requested_as_of: Option<Date>, tx: TransactionTime) -> QueryStamp {
        QueryStamp {
            valid_as_of: ValidAsOf::new(requested_as_of.unwrap_or(self.default_as_of)),
            transaction_time: tx,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::{date, datetime};

    #[test]
    fn uses_default_when_no_stichtag_given() {
        let r = TemporalResolver::new(date!(2024 - 01 - 01));
        let stamp = r.stamp_at(None, TransactionTime::new(datetime!(2026-06-01 09:00 UTC)));
        assert_eq!(stamp.valid_as_of().to_string(), "2024-01-01");
    }

    #[test]
    fn honours_requested_stichtag() {
        let r = TemporalResolver::new(date!(2024 - 01 - 01));
        let stamp = r.stamp_at(
            Some(date!(2019 - 07 - 01)),
            TransactionTime::new(datetime!(2026-06-01 09:00 UTC)),
        );
        assert_eq!(stamp.valid_as_of().to_string(), "2019-07-01");
    }

    #[test]
    fn stamp_carries_into_provenance() {
        let r = TemporalResolver::new(date!(2024 - 01 - 01));
        let stamp = r.stamp_at(
            Some(date!(2020 - 01 - 01)),
            TransactionTime::new(datetime!(2026-06-01 09:00 UTC)),
        );
        let prov = stamp.into_provenance(Eli::new("eli/cc/1999/404").unwrap());
        assert_eq!(prov.eli.as_str(), "eli/cc/1999/404");
        assert_eq!(prov.valid_as_of.to_string(), "2020-01-01");
    }
}
