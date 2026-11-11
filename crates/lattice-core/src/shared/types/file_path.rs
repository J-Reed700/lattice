use super::DomainTypeError;
use crate::shared::path_safety::reject_parent_components;
use serde::{Deserialize, Serialize};
use std::fmt;

use std::path::{Path, PathBuf};

/// Validated file path
///
/// Provides a type-safe wrapper around PathBuf with validation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ValidatedFilePath(PathBuf);

impl ValidatedFilePath {
    /// Create a new validated file path
    ///
    /// # Errors
    ///
    /// Returns an error if the path is empty or invalid
    pub fn new(path: PathBuf) -> Result<Self, DomainTypeError> {
        if path.as_os_str().is_empty() {
            return Err(DomainTypeError::EmptyValue);
        }

        reject_parent_components(&path)
            .map_err(|reason| DomainTypeError::InvalidId(format!("Path {reason}")))?;

        Ok(Self(path))
    }

    /// Get the inner PathBuf
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Get the path as a string slice
    pub fn as_str(&self) -> &str {
        self.0.to_str().unwrap_or("")
    }

    /// Convert to PathBuf
    pub fn into_inner(self) -> PathBuf {
        self.0
    }
}

impl fmt::Display for ValidatedFilePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

impl From<ValidatedFilePath> for PathBuf {
    fn from(vfp: ValidatedFilePath) -> Self {
        vfp.0
    }
}
