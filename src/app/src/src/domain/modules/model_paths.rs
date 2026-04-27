//! Model Paths Value Object
//!
//! Centralized path management for model storage with automatic migration.

use crate::shared::error::AppError;
use std::path::{Path, PathBuf};

/// Value object for model storage paths.
///
/// Provides a unified location (`~/.cache/lattice/models/`) with
/// automatic detection and migration from legacy locations.
///
/// # Security
///
/// - Validates model_id to prevent directory traversal (CWE-22)
/// - Rejects paths containing `..`, `/`, or `\`
///
/// # Example
///
/// ```rust,no_run
/// use vault_desktop::domain::model_paths::ModelPaths;
///
/// let paths = ModelPaths::new("phi-3-mini")?;
/// println!("Unified path: {}", paths.unified_path().display());
///
/// if let Some(legacy) = paths.legacy_path() {
///     println!("Legacy path exists: {}", legacy.display());
/// }
/// # Ok::<(), vault_desktop::shared::error::AppError>(())
/// ```
#[derive(Debug, Clone)]
pub struct ModelPaths {
    model_id: String,
    unified_path: PathBuf,
    legacy_path: Option<PathBuf>,
}

impl ModelPaths {
    /// Create new model paths for the given model ID.
    ///
    /// # Arguments
    ///
    /// * `model_id` - Model identifier (validated for security)
    ///
    /// # Returns
    ///
    /// * `Ok(ModelPaths)` - Valid paths
    /// * `Err(AppError::InvalidInput)` - Invalid model_id (path traversal, empty, etc.)
    ///
    /// # Security
    ///
    /// Prevents directory traversal by rejecting:
    /// - `..` sequences
    /// - `/` or `\` path separators
    /// - Empty strings
    pub fn new(model_id: &str) -> Result<Self, AppError> {
        Self::validate_model_id(model_id)?;

        let unified_path = Self::build_unified_path(model_id)?;
        let legacy_path = Self::detect_legacy_path(model_id)?;

        Ok(Self {
            model_id: model_id.to_string(),
            unified_path,
            legacy_path,
        })
    }

    /// Get the unified storage path.
    ///
    /// This is the canonical location where models should be stored:
    /// `~/.cache/lattice/models/{model_id}/`
    pub fn unified_path(&self) -> &Path {
        &self.unified_path
    }

    /// Get the legacy storage path if it exists.
    ///
    /// Returns `Some(path)` if the old `~/.lattice/models/{model_id}/` exists.
    pub fn legacy_path(&self) -> Option<&Path> {
        self.legacy_path.as_deref()
    }

    /// Get the model ID.
    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Check if migration is needed (legacy location exists).
    pub fn needs_migration(&self) -> bool {
        self.legacy_path.is_some()
    }

    /// Get the full path for a model file.
    ///
    /// Constructs the complete file path by appending the filename
    /// to the unified model directory path.
    ///
    /// # Arguments
    ///
    /// * `filename` - The filename to append to the model directory
    ///
    /// # Returns
    ///
    /// Full path to the file: `~/.cache/lattice/models/{model_id}/{filename}`
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use vault_desktop::domain::model_paths::ModelPaths;
    ///
    /// let paths = ModelPaths::new("phi-3-mini")?;
    /// let file_path = paths.file_path("model.gguf");
    /// // Returns: ~/.cache/lattice/models/phi-3-mini/model.gguf
    /// # Ok::<(), vault_desktop::shared::error::AppError>(())
    /// ```
    pub fn file_path(&self, filename: &str) -> PathBuf {
        self.unified_path.join(filename)
    }

    /// Validate model ID for security (prevent directory traversal).
    fn validate_model_id(model_id: &str) -> Result<(), AppError> {
        if model_id.is_empty() {
            return Err(AppError::InvalidInput(
                "Model ID cannot be empty".to_string(),
            ));
        }

        if model_id.contains("..") || model_id.contains('/') || model_id.contains('\\') {
            return Err(AppError::InvalidInput(
                "Invalid model ID: contains path separators or traversal".to_string(),
            ));
        }

        Ok(())
    }

    /// Build the unified storage path: `~/.cache/lattice/models/{model_id}`
    fn build_unified_path(model_id: &str) -> Result<PathBuf, AppError> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| AppError::InvalidConfig("Cannot determine home directory".into()))?;

        let mut path = PathBuf::from(home);
        path.push(".cache");
        path.push("lattice");
        path.push("models");
        path.push(model_id);

        Ok(path)
    }

    /// Detect if legacy path exists: `~/.lattice/models/{model_id}`
    fn detect_legacy_path(model_id: &str) -> Result<Option<PathBuf>, AppError> {
        let home = dirs::home_dir()
            .ok_or_else(|| AppError::InvalidInput("Home directory not found".to_string()))?;

        let legacy_path = home.join(".lattice").join("models").join(model_id);

        if legacy_path.exists() && legacy_path.is_dir() {
            Ok(Some(legacy_path))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_model_id_valid() {
        assert!(ModelPaths::validate_model_id("phi-3-mini").is_ok());
        assert!(ModelPaths::validate_model_id("mistral-7b").is_ok());
        assert!(ModelPaths::validate_model_id("model_123").is_ok());
    }

    #[test]
    fn test_validate_model_id_invalid() {
        // Directory traversal
        assert!(ModelPaths::validate_model_id("../etc/passwd").is_err());
        assert!(ModelPaths::validate_model_id("model/../../../etc").is_err());

        // Path separators
        assert!(ModelPaths::validate_model_id("path/to/model").is_err());
        assert!(ModelPaths::validate_model_id("path\\to\\model").is_err());

        // Empty
        assert!(ModelPaths::validate_model_id("").is_err());
    }

    #[test]
    fn test_new_model_paths() {
        let paths = ModelPaths::new("phi-3-mini").unwrap();

        assert_eq!(paths.model_id(), "phi-3-mini");
        assert!(paths
            .unified_path()
            .to_string_lossy()
            .contains(".cache/lattice/models/phi-3-mini"));
    }

    #[test]
    fn test_unified_path_structure() {
        let paths = ModelPaths::new("test-model").unwrap();
        let path_str = paths.unified_path().to_string_lossy();

        assert!(path_str.ends_with("test-model"));
        assert!(path_str.contains(".cache"));
        assert!(path_str.contains("lattice"));
        assert!(path_str.contains("models"));
    }
}
