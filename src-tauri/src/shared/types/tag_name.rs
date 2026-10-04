use super::DomainTypeError;
use derive_more::Display;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub(super) const MAX_TAG_NAME_LENGTH: usize = 50;

/// Validated tag name with length constraints
///
/// Tag names must be:
/// - Non-empty
/// - Maximum 50 characters
/// - Contain valid characters
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[serde(transparent)]
pub struct TagName(String);

impl TagName {
    /// Create a new validated tag name
    ///
    /// # Errors
    ///
    /// Returns an error if the name is empty or too long
    pub fn new(name: String) -> Result<Self, DomainTypeError> {
        if name.is_empty() {
            return Err(DomainTypeError::EmptyValue);
        }

        if name.len() > MAX_TAG_NAME_LENGTH {
            return Err(DomainTypeError::ValueTooLong(
                name.len(),
                MAX_TAG_NAME_LENGTH,
            ));
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
