use super::DomainTypeError;
use derive_more::Display;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Validated tag name
///
/// Tag names must be:
/// - Non-empty
/// - Contain valid characters
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[serde(transparent)]
pub struct TagName(String);

impl TagName {
    /// Create a new validated tag name
    ///
    /// # Errors
    ///
    /// Returns an error if the name is empty
    pub fn new(name: String) -> Result<Self, DomainTypeError> {
        if name.is_empty() {
            return Err(DomainTypeError::EmptyValue);
        }

        Ok(Self(name))
    }

    /// Get the inner string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for TagName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for TagName {
    type Err = DomainTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s.to_string())
    }
}
