//! fedlex-core - geteilte Kern-Typen des mcp-fedlex-Workspace.
//!
//! Diese Crate bündelt die Typen, die alle Schichten teilen, und kodiert die
//! beiden wichtigsten Invarianten direkt im Typsystem.
//!
//! - [`Response<T>`] erzwingt die Herkunfts-Pflicht aus ADR-004 (Provenance-Gate).
//! - [`Sensitive<T>`] verhindert PII-Lecks aus ADR-001 (redactender Newtype).
//!
//! Dazu die fachlichen Grundtypen [`Eli`]/[`Ecli`] (Föderations-URI-Schema) und
//! die bi-temporalen [`ValidAsOf`]/[`TransactionTime`].

#![forbid(unsafe_code)]

pub mod eid;
pub mod eli;
pub mod provenance;
pub mod response;
pub mod sensitive;
pub mod temporal;

pub use eid::normalize_eid;
pub use eli::{Ecli, Eli, IdError};
pub use provenance::{Provenance, ProvenanceKind};

pub use response::Response;
pub use sensitive::Sensitive;
pub use temporal::{TransactionTime, ValidAsOf};

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::{date, datetime};

    /// Integrations-Smoke über die öffentliche API: ELI -> Provenance ->
    /// Response, alles über die Re-Exports erreichbar.
    #[test]
    fn public_api_composes() {
        let prov = Provenance::new(
            Eli::new("eli/cc/1999/404").unwrap(),
            ValidAsOf::new(date!(2020 - 01 - 01)),
            TransactionTime::new(datetime!(2026-06-01 09:00 UTC)),
        );
        let resp = Response::new(Sensitive::new("Mandant X"), prov);
        // PII bleibt auch in der Response redacted.
        assert_eq!(format!("{:?}", resp.data()), "[REDACTED]");
        assert_eq!(resp.provenance().eli.as_str(), "eli/cc/1999/404");
    }
}
