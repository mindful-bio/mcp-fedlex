//! Antwort-Hülle `Response<T>` als strukturelles Provenance-Gate (ADR-004).
//!
//! Der Kern der Entscheidung. Eine Tool-Antwort kann nicht ohne Herkunft
//! existieren, weil das einzige Konstrukt, das Daten nach aussen trägt, eine
//! [`Provenance`] verlangt. Das private Feld verhindert, dass ausserhalb dieses
//! Moduls ein `Response` ohne Gate gebaut wird. Damit ist die Pflicht aus
//! ADR-004 eine Compile-Zeit-Garantie statt einer Konvention.

use crate::provenance::Provenance;
use serde::{Deserialize, Serialize};

/// Eine Tool-Antwort mit zwingender Herkunft (ADR-004).
///
/// `data` ist die eigentliche Nutzlast, `provenance` ihre Rückführbarkeit.
/// Nur über [`Response::new`] konstruierbar, daher gibt es kein nacktes `T`
/// als Tool-Antwort.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response<T> {
    data: T,
    provenance: Provenance,
}

impl<T> Response<T> {
    /// Verpackt eine Nutzlast zusammen mit ihrer Pflicht-Herkunft.
    pub fn new(data: T, provenance: Provenance) -> Self {
        Self { data, provenance }
    }

    /// Liest die Nutzlast.
    pub fn data(&self) -> &T {
        &self.data
    }

    /// Liest die Herkunft.
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// Zerlegt die Hülle in Nutzlast und Herkunft.
    pub fn into_parts(self) -> (T, Provenance) {
        (self.data, self.provenance)
    }

    /// Bildet die Nutzlast ab und behält die Herkunft unverändert bei.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Response<U> {
        Response {
            data: f(self.data),
            provenance: self.provenance,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eli::Eli;
    use crate::temporal::{TransactionTime, ValidAsOf};
    use time::macros::{date, datetime};

    fn provenance() -> Provenance {
        Provenance::new(
            Eli::new("eli/cc/1999/404").unwrap(),
            ValidAsOf::new(date!(2020 - 01 - 01)),
            TransactionTime::new(datetime!(2026-06-01 09:00 UTC)),
        )
    }

    #[test]
    fn response_wraps_payload_and_provenance() {
        let r = Response::new("Art. 1 BV", provenance());
        assert_eq!(*r.data(), "Art. 1 BV");
        assert_eq!(r.provenance().eli.as_str(), "eli/cc/1999/404");
    }

    #[test]
    fn response_map_preserves_provenance() {
        let r = Response::new(3u32, provenance());
        let mapped = r.map(|n| n * 2);
        assert_eq!(*mapped.data(), 6);
        assert_eq!(mapped.provenance().eli.as_str(), "eli/cc/1999/404");
    }

    #[test]
    fn response_into_parts_roundtrips() {
        let r = Response::new(vec![1, 2, 3], provenance());
        let (data, prov) = r.into_parts();
        assert_eq!(data, vec![1, 2, 3]);
        assert_eq!(prov, provenance());
    }

    // ADR-004-Nachweis: `Response` hat private Felder, daher ist es ausserhalb
    // dieses Crates NICHT möglich, eine Antwort ohne `Provenance` zu bauen.
    // Der einzige Konstruktor `Response::new` erzwingt das Gate strukturell.
    // Ein hypothetisches `Response { data }` ohne provenance kompiliert nicht.
    #[test]
    fn response_requires_provenance_by_construction() {
        // Dies kompiliert nur MIT Provenance:
        let _ok = Response::new((), provenance());
        // Ein nacktes `Response { data: () }` wäre ein Compile-Fehler
        // (privates Feld + fehlendes Pflichtfeld) und ist daher nicht testbar
        // als Laufzeitfall, sondern durch den Typ ausgeschlossen.
    }
}
