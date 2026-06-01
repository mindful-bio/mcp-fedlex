//! Bi-temporale Zeitstempel.
//!
//! mcp-fedlex unterscheidet zwei Zeitachsen. `ValidAsOf` ist die Gültigkeitszeit
//! (welche Fassung einer Norm galt an einem Stichtag), `TransactionTime` ist die
//! Systemzeit (wann die Information erfasst wurde, für rückwirkende Korrekturen).
//! Beide sind dünne, typsichere Hüllen um ein Datum bzw. einen Zeitpunkt, damit
//! die beiden Achsen nicht versehentlich vertauscht werden.

use serde::{Deserialize, Serialize};
use std::fmt;
use time::{Date, OffsetDateTime};

/// Gültigkeitszeit. Der Stichtag, dessen konsolidierte Fassung gemeint ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ValidAsOf(pub Date);

impl ValidAsOf {
    /// Erzeugt eine Gültigkeitszeit aus einem Datum.
    pub fn new(date: Date) -> Self {
        Self(date)
    }

    /// Liefert das zugrunde liegende Datum.
    pub fn date(&self) -> Date {
        self.0
    }
}

impl fmt::Display for ValidAsOf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Date> for ValidAsOf {
    fn from(date: Date) -> Self {
        Self(date)
    }
}

/// Systemzeit. Der Zeitpunkt, zu dem ein Fakt erfasst oder korrigiert wurde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TransactionTime(pub OffsetDateTime);

impl TransactionTime {
    /// Erzeugt eine Systemzeit aus einem Zeitpunkt.
    pub fn new(at: OffsetDateTime) -> Self {
        Self(at)
    }

    /// Aktuelle Systemzeit (UTC).
    pub fn now() -> Self {
        Self(OffsetDateTime::now_utc())
    }

    /// Liefert den zugrunde liegenden Zeitpunkt.
    pub fn instant(&self) -> OffsetDateTime {
        self.0
    }
}

impl From<OffsetDateTime> for TransactionTime {
    fn from(at: OffsetDateTime) -> Self {
        Self(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::{date, datetime};

    #[test]
    fn valid_as_of_orders_chronologically() {
        let early = ValidAsOf::new(date!(1999 - 01 - 01));
        let late = ValidAsOf::new(date!(2020 - 06 - 15));
        assert!(early < late);
    }

    #[test]
    fn valid_as_of_displays_iso() {
        let v = ValidAsOf::new(date!(2002 - 12 - 31));
        assert_eq!(v.to_string(), "2002-12-31");
    }

    #[test]
    fn transaction_time_roundtrips_through_serde() {
        let tt = TransactionTime::new(datetime!(2026-06-01 10:30 UTC));
        let json = serde_json::to_string(&tt).unwrap();
        let back: TransactionTime = serde_json::from_str(&json).unwrap();
        assert_eq!(back, tt);
    }

    #[test]
    fn transaction_time_now_is_monotonic_enough() {
        let a = TransactionTime::now();
        let b = TransactionTime::now();
        assert!(b.instant() >= a.instant());
    }
}
