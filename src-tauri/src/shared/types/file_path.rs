use super::DomainTypeError;
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

        // Check for path traversal attacks (CWE-22)
        // Only reject ".." as a path component, not in file/directory names
        for component in path.components() {
            if let std::path::Component::ParentDir = component {
                return Err(DomainTypeError::InvalidId(
                    "Path contains parent directory reference '..' which is not allowed"
                        .to_string(),
                ));
            }
        }

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
