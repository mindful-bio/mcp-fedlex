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

/// Art der Herkunft (ADR-004 / ADR-006).
///
/// Die Unterscheidung ist **strukturell, nicht konventionell**: Ein Konsument
/// (`syllogismus-fedlex`/`ansV`) kann einen Hinweis nicht versehentlich als
/// Norm-Beleg verbuchen, weil der Typ es ausweist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProvenanceKind {
    /// Belegte Aussage. Geht strukturell auf eine konkrete Norm-Fassung zurück
    /// (ADR-004-Regelfall). Default, damit bestehende Antworten unverändert
    /// als `norm` serialisieren.
    #[default]
    Norm,
    /// Hinweis/Kandidat (Discovery, ADR-006). „Zum Stichtag X als Kandidat
    /// gefunden", **kein** Beleg. Darf vom Reasoner nicht als Beleg zählen.
    Hint,
}

/// Strukturelle Herkunft einer gelieferten Aussage (ADR-004).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// ELI der Quelle, auf die die Aussage zurückgeht.
    pub eli: Eli,
    /// Gültigkeitszeit. Welche Fassung galt am Stichtag.
    pub valid_as_of: ValidAsOf,
    /// Systemzeit. Wann die zugrunde liegende Information erfasst wurde.
    pub transaction_time: TransactionTime,
    /// Norm-Beleg oder Discovery-Hinweis (ADR-006). Bei fehlendem Feld in
    /// älteren Payloads wird `norm` angenommen (Abwärtskompatibilität).
    #[serde(default)]
    pub kind: ProvenanceKind,
}

impl Provenance {
    /// Erzeugt eine vollständige **Norm**-Herkunft. Alle drei Zeit-/ELI-Felder
    /// sind Pflicht; `kind` ist `Norm` (ADR-004-Regelfall).
    pub fn new(eli: Eli, valid_as_of: ValidAsOf, transaction_time: TransactionTime) -> Self {
        Self {
            eli,
            valid_as_of,
            transaction_time,
            kind: ProvenanceKind::Norm,
        }
    }

    /// Erzeugt eine **Hinweis**-Herkunft (Discovery, ADR-006). Gleiche
    /// Pflichtfelder wie [`Self::new`], aber `kind` ist `Hint` — der Treffer
    /// ist ein Kandidat zum Stichtag, kein Beleg.
    pub fn hint(eli: Eli, valid_as_of: ValidAsOf, transaction_time: TransactionTime) -> Self {
        Self {
            eli,
            valid_as_of,
            transaction_time,
            kind: ProvenanceKind::Hint,
        }
    }

    /// Ob diese Herkunft ein Beleg (Norm) ist. Hinweise liefern `false`.
    pub fn is_norm(&self) -> bool {
        matches!(self.kind, ProvenanceKind::Norm)
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

    // ADR-006: Norm ist Default und serialisiert als "norm".
    #[test]
    fn norm_is_default_kind_and_serializes_as_norm() {
        let p = sample();
        assert_eq!(p.kind, ProvenanceKind::Norm);
        assert!(p.is_norm());
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains(r#""kind":"norm""#), "got: {json}");
    }

    // ADR-006: Ein Hinweis weist sich strukturell als "hint" aus und ist kein Beleg.
    #[test]
    fn hint_carries_hint_kind_and_is_not_norm() {
        let p = Provenance::hint(
            Eli::new("eli/cc/2017/762").unwrap(),
            ValidAsOf::new(date!(2020 - 01 - 01)),
            TransactionTime::new(datetime!(2026-06-01 09:00 UTC)),
        );
        assert_eq!(p.kind, ProvenanceKind::Hint);
        assert!(!p.is_norm(), "ein Hinweis darf nicht als Norm-Beleg zählen");
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains(r#""kind":"hint""#), "got: {json}");
    }

    // Abwärtskompatibilität: ältere Payloads ohne `kind` werden als Norm gelesen.
    // Das Fixture wird aus einer echten Serialisierung gebaut, dann das `kind`-
    // Feld entfernt — so bleibt es unabhängig vom Wire-Format der Zeitfelder.
    #[test]
    fn missing_kind_deserializes_as_norm() {
        let mut value = serde_json::to_value(sample()).unwrap();
        value.as_object_mut().unwrap().remove("kind");
        assert!(value.get("kind").is_none(), "Fixture muss kind-los sein");
        let p: Provenance = serde_json::from_value(value).unwrap();
        assert_eq!(p.kind, ProvenanceKind::Norm);
    }
}
