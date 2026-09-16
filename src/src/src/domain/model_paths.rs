//! Model Paths Value Object
//!
//! Centralized path management for model storage.

use crate::shared::error::AppError;
use std::path::{Path, PathBuf};

/// Value object for model storage paths.
///
/// Every model lives under the single unified location
/// (`~/.cache/lattice/models/`).
///
/// # Security
///
/// - Validates model_id to prevent directory traversal (CWE-22)
/// - Rejects paths containing `..`, `/`, or `\`
///
/// # Example
///
/// ```rust,no_run
/// use lattice::domain::model_paths::ModelPaths;
///
/// let paths = ModelPaths::new("phi-3-mini")?;
/// println!("Unified path: {}", paths.unified_path().display());
/// # Ok::<(), lattice::shared::error::AppError>(())
/// ```
#[derive(Debug, Clone)]
pub struct ModelPaths {
    model_id: String,
    unified_path: PathBuf,
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

        Ok(Self {
            model_id: model_id.to_string(),
            unified_path,
        })
    }

    /// Get the unified storage path.
    ///
    /// This is the canonical location where models should be stored:
    /// `~/.cache/lattice/models/{model_id}/`
    pub fn unified_path(&self) -> &Path {
        &self.unified_path
    }

    /// Get the model ID.
    pub fn model_id(&self) -> &str {
        &self.model_id
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
    /// use lattice::domain::model_paths::ModelPaths;
    ///
    /// let paths = ModelPaths::new("phi-3-mini")?;
    /// let file_path = paths.file_path("model.gguf")?;
    /// // Returns: ~/.cache/lattice/models/phi-3-mini/model.gguf
    /// # Ok::<(), lattice::shared::error::AppError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `AppError::Security` if `filename` is not a bare file name.
    ///
    /// This is fallible on purpose. `filename` reaches here as the
    /// base64url-decoded second segment of a frontend-supplied download id,
    /// and `Path::join` **discards the base when the joined component is
    /// absolute** — so `paths.file_path("/Users/example/.zshenv")` used to
    /// return exactly that path, dropping the models root entirely and
    /// letting a download write anywhere the user can write.
    pub fn file_path(&self, filename: &str) -> Result<PathBuf, AppError> {
        crate::shared::path_confinement::validate_bare_filename(filename)?;

        let joined = self.unified_path.join(filename);

        // Belt and braces: even with a validated bare name, confirm the
        // result is still under the models root before handing it to a writer.
        crate::shared::path_confinement::confine_to_root(&Self::models_root()?, &joined)
    }

    /// Resolve an internally sourced manifest-relative path under this model.
    ///
    /// Unlike `file_path`, this accepts nested entries such as
    /// `onnx/model.onnx`. It is intentionally separate because `file_path`
    /// accepts a frontend-decoded *bare filename*, while this method is for a
    /// trusted manifest identity that still needs traversal and symlink
    /// confinement.
    pub fn manifest_file_path(&self, relative_path: &str) -> Result<PathBuf, AppError> {
        use std::path::Component;

        let relative = Path::new(relative_path);
        if relative_path.is_empty()
            || relative_path.contains('\0')
            || relative_path.contains('\\')
            || relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(AppError::Security(format!(
                "Invalid model manifest path: {}",
                relative_path
            )));
        }

        let joined = self.unified_path.join(relative);
        crate::shared::path_confinement::confine_to_root(&Self::models_root()?, &joined)
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
    /// The directory every downloaded model must live under:
    /// `~/.cache/lattice/models`.
    ///
    /// Exposed so command handlers can confine caller-supplied destinations to
    /// it. Creates the directory if absent, because confinement checks need a
    /// canonicalizable root and a fresh install has none.
    pub fn models_root() -> Result<PathBuf, AppError> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| AppError::InvalidConfig("Cannot determine home directory".into()))?;

        let root = PathBuf::from(home)
            .join(".cache")
            .join("lattice")
            .join("models");

        if !root.exists() {
            std::fs::create_dir_all(&root).map_err(|e| {
                AppError::FileSystem(format!(
                    "Failed to create models directory {}: {}",
                    root.display(),
                    e
                ))
            })?;
        }

        Ok(root)
    }

    fn build_unified_path(model_id: &str) -> Result<PathBuf, AppError> {
        Ok(Self::models_root()?.join(model_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Path::join` silently drops the base when given an absolute component,
    /// so an absolute "filename" used to escape the models root completely.
    /// The filename arrives base64url-decoded from a frontend-supplied
    /// download id, so it is fully attacker-controlled.
    #[test]
    fn file_path_rejects_absolute_filename() {
        let paths = ModelPaths::new("phi-3-mini").expect("valid model id");
        assert!(
            paths.file_path("/Users/example/.zshenv").is_err(),
            "an absolute filename must not escape the models root"
        );
    }

    #[test]
    fn file_path_rejects_separators_and_traversal() {
        let paths = ModelPaths::new("phi-3-mini").expect("valid model id");
        assert!(paths.file_path("../../.zshenv").is_err());
        assert!(paths.file_path("sub/dir.gguf").is_err());
        assert!(paths.file_path("sub\\dir.gguf").is_err());
        assert!(paths.file_path("").is_err());
        assert!(paths.file_path("..").is_err());
    }

    #[test]
    fn file_path_accepts_a_plain_filename_and_stays_under_the_root() {
        let paths = ModelPaths::new("phi-3-mini").expect("valid model id");
        let resolved = paths.file_path("model.gguf").expect("plain name is fine");
        let root = ModelPaths::models_root()
            .expect("models root")
            .canonicalize()
            .expect("canonical root");
        assert!(resolved.starts_with(root));
        assert!(resolved.ends_with("model.gguf"));
    }

    #[test]
    fn manifest_file_path_allows_confined_subdirectories() {
        let paths = ModelPaths::new("phi-3-mini").expect("valid model id");
        let resolved = paths
            .manifest_file_path("onnx/model.onnx")
            .expect("safe nested manifest path");
        assert!(resolved.starts_with(paths.unified_path()));
        assert!(resolved.ends_with(Path::new("onnx/model.onnx")));
    }

    #[test]
    fn manifest_file_path_rejects_escapes() {
        let paths = ModelPaths::new("phi-3-mini").expect("valid model id");
        assert!(paths.manifest_file_path("../outside.gguf").is_err());
        assert!(paths.manifest_file_path("/tmp/outside.gguf").is_err());
        assert!(paths.manifest_file_path("sub\\outside.gguf").is_err());
        assert!(paths.manifest_file_path("").is_err());
    }

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
