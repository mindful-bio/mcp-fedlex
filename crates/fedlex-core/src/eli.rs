//! European Legislation Identifier (ELI) und European Case Law Identifier (ECLI).
//!
//! ELI ist das gemeinsame URI-Schema, das mcp-fedlex von Bund über Kantone bis
//! zur EU trägt. Beide Typen sind validierende Newtypes über `String`, damit ein
//! leerer oder offensichtlich kaputter Identifier nicht durch das System wandert.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Fehler beim Parsen eines [`Eli`] oder [`Ecli`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    /// Der übergebene Identifier war leer oder nur Whitespace.
    #[error("identifier must not be empty")]
    Empty,
    /// Der Identifier trug nicht das erwartete Präfix.
    #[error("expected prefix `{expected}`, got `{got}`")]
    WrongPrefix {
        /// Erwartetes Präfix.
        expected: &'static str,
        /// Tatsächlich gefundener Anfang.
        got: String,
    },
}

/// European Legislation Identifier, z.B. `eli/cc/1999/404` (konsolidierte Fassung).
///
/// Validierender Newtype. Der Inhalt wird getrimmt und muss mit `eli/` beginnen.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Eli(String);

impl Eli {
    const PREFIX: &'static str = "eli/";

    /// Erzeugt einen ELI aus einem beliebigen String und validiert Präfix/Inhalt.
    pub fn new(raw: impl Into<String>) -> Result<Self, IdError> {
        let trimmed = raw.into().trim().to_string();
        if trimmed.is_empty() {
            return Err(IdError::Empty);
        }
        if !trimmed.starts_with(Self::PREFIX) {
            return Err(IdError::WrongPrefix {
                expected: Self::PREFIX,
                got: trimmed.chars().take(Self::PREFIX.len()).collect(),
            });
        }
        Ok(Self(trimmed))
    }

    /// Liefert die rohe ELI-Zeichenkette.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Eli {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Eli {
    type Err = IdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl TryFrom<String> for Eli {
    type Error = IdError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Eli> for String {
    fn from(value: Eli) -> Self {
        value.0
    }
}

/// European Case Law Identifier, z.B. `ecli/CH/bger/2020/...` (Rechtsprechung).
///
/// Validierender Newtype analog zu [`Eli`], mit Präfix `ecli/`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Ecli(String);

impl Ecli {
    const PREFIX: &'static str = "ecli/";

    /// Erzeugt einen ECLI aus einem beliebigen String und validiert Präfix/Inhalt.
    pub fn new(raw: impl Into<String>) -> Result<Self, IdError> {
        let trimmed = raw.into().trim().to_string();
        if trimmed.is_empty() {
            return Err(IdError::Empty);
        }
        if !trimmed.starts_with(Self::PREFIX) {
            return Err(IdError::WrongPrefix {
                expected: Self::PREFIX,
                got: trimmed.chars().take(Self::PREFIX.len()).collect(),
            });
        }
        Ok(Self(trimmed))
    }

    /// Liefert die rohe ECLI-Zeichenkette.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Ecli {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Ecli {
    type Err = IdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl TryFrom<String> for Ecli {
    type Error = IdError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Ecli> for String {
    fn from(value: Ecli) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eli_accepts_valid_and_trims() {
        let eli = Eli::new("  eli/cc/1999/404  ").unwrap();
        assert_eq!(eli.as_str(), "eli/cc/1999/404");
        assert_eq!(eli.to_string(), "eli/cc/1999/404");
    }

    #[test]
    fn eli_rejects_empty() {
        assert_eq!(Eli::new("   "), Err(IdError::Empty));
    }

    #[test]
    fn eli_rejects_wrong_prefix() {
        assert!(matches!(
            Eli::new("https://example.org/x"),
            Err(IdError::WrongPrefix { .. })
        ));
    }

    #[test]
    fn eli_roundtrips_through_serde() {
        let eli = Eli::new("eli/cc/2002/199").unwrap();
        let json = serde_json::to_string(&eli).unwrap();
        assert_eq!(json, "\"eli/cc/2002/199\"");
        let back: Eli = serde_json::from_str(&json).unwrap();
        assert_eq!(back, eli);
    }

    #[test]
    fn eli_serde_rejects_invalid() {
        let err = serde_json::from_str::<Eli>("\"not-an-eli\"");
        assert!(err.is_err());
    }

    #[test]
    fn ecli_accepts_valid() {
        let ecli = Ecli::new("ecli/CH/bger/2020/4A_1_2020").unwrap();
        assert_eq!(ecli.as_str(), "ecli/CH/bger/2020/4A_1_2020");
    }

    #[test]
    fn ecli_rejects_wrong_prefix() {
        assert!(matches!(
            Ecli::new("eli/cc/1999/404"),
            Err(IdError::WrongPrefix { .. })
        ));
    }
}
