//! Fehlertyp der Brücken-Schicht.

use fedlex_akn::AknError;
use fedlex_jolux::JoluxError;

/// Fehler beim Beschaffen oder Parsen eines AKN-Dokuments über die Brücke.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    /// Die JOLux-Auflösung (Konsolidierung/Manifestation) schlug fehl —
    /// inklusive `NotFound`, wenn zum Stichtag kein XML existiert (J14.2,
    /// Manifestationen erst ab ~2021).
    #[error(transparent)]
    Jolux(#[from] JoluxError),
    /// Das heruntergeladene XML liess sich nicht als AKN parsen.
    #[error(transparent)]
    Akn(#[from] AknError),
    /// Transportfehler beim XML-Download.
    #[error("XML-Download fehlgeschlagen: {0}")]
    Download(String),
}
