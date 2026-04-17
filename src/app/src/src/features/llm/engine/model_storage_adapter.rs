//! Model storage adapter implementation.
//!
//! Production adapter using filesystem for model storage.

use crate::application::ports::model_storage::{DownloadedModel, ModelStoragePort};
use crate::domain::model_file_validator::ModelFileValidator;
use crate::domain::model_paths::ModelPaths;
use crate::domain::ports::file_access::{ChecksumService, FileSystemAccess};
use crate::shared::error::AppError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Filesystem-based model storage.
///
/// Stores models in `~/.cache/recall/models/` directory (unified location).
/// Automatically detects and migrates from legacy `~/.recall/models/` location.
/// Each model is stored in its own subdirectory.
pub struct FilesystemModelStorage {
    models_dir: PathBuf,
    validator: ModelFileValidator,
}

impl FilesystemModelStorage {
    /// Create a new filesystem model storage.
    ///
    /// Uses unified path: `~/.cache/recall/models/`
    /// Creates the directory if it doesn't exist.
    pub fn new(
        checksum_service: Arc<dyn ChecksumService>,
        file_system_access: Arc<dyn FileSystemAccess>,
    ) -> Result<Self, AppError> {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| AppError::InvalidConfig("Cannot determine home directory".into()))?;

        let mut models_dir = PathBuf::from(home);
        models_dir.push(".cache");
        models_dir.push("recall");
        models_dir.push("models");

        std::fs::create_dir_all(&models_dir)?;

        Ok(Self {
            models_dir,
            validator: ModelFileValidator::new(checksum_service, file_system_access),
        })
    }

    /// Migrate model from legacy location to unified location.
    ///
    /// # Arguments
    ///
    /// * `model_id` - Model identifier
    /// * `legacy_path` - Path to legacy location
    /// * `unified_path` - Path to new unified location
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Migration successful
    /// * `Err(AppError)` - Migration failed
    fn migrate_from_legacy(
        &self,
        model_id: &str,
        legacy_path: &Path,
        unified_path: &Path,
    ) -> Result<(), AppError> {
        info!(
            model_id = %model_id,
            from = %legacy_path.display(),
            to = %unified_path.display(),
            "Migrating model from legacy location"
        );

        // Ensure parent directory exists
        if let Some(parent) = unified_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Move directory
        std::fs::rename(legacy_path, unified_path).map_err(|e| {
            AppError::FileSystem(format!(
                "Failed to migrate model {} from {} to {}: {}",
                model_id,
                legacy_path.display(),
                unified_path.display(),
                e
            ))
        })?;

        info!(
            model_id = %model_id,
            "Model migration completed successfully"
        );

        Ok(())
    }

    /// Sanitize model ID to prevent directory traversal attacks.
    fn sanitize_model_id(model_id: &str) -> Result<String, AppError> {
        if model_id.contains("..") || model_id.contains('/') || model_id.contains('\\') {
            return Err(AppError::InvalidInput(
                "Invalid model ID: contains path separators or traversal".to_string(),
            ));
        }
        if model_id.is_empty() {
            return Err(AppError::InvalidInput(
                "Model ID cannot be empty".to_string(),
            ));
        }
        Ok(model_id.to_string())
    }

    /// Calculate total size of model directory in bytes.
    #[allow(clippy::only_used_in_recursion)]
    fn get_model_size(&self, path: &Path) -> Result<u64, AppError> {
        let mut total_bytes = 0u64;

        if !path.exists() {
            return Ok(0);
        }

        if path.is_file() {
            return Ok(path.metadata()?.len());
        }

        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_file() {
                    total_bytes += entry.metadata()?.len();
                } else if entry_path.is_dir() {
                    // Recurse into subdirectories
                    total_bytes += self.get_model_size(&entry_path)?;
                }
            }
        }

        Ok(total_bytes)
    }

    /// Get the last modified time of a directory.
    fn get_modified_time(&self, path: &Path) -> Result<DateTime<Utc>, AppError> {
        let metadata = std::fs::metadata(path)?;
        let modified = metadata.modified()?;
        Ok(modified.into())
    }
}

#[async_trait]
impl ModelStoragePort for FilesystemModelStorage {
    async fn list_models(&self) -> Result<Vec<DownloadedModel>, AppError> {
        let mut models = Vec::new();

        if !self.models_dir.exists() {
            return Ok(models);
        }

        let entries = std::fs::read_dir(&self.models_dir)?;

        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() {
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    let model_id = file_name.to_string();
                    let size_bytes = self.get_model_size(&path)?;
                    let downloaded_at = self.get_modified_time(&path)?;

                    models.push(DownloadedModel {
                        model_id,
                        path,
                        size_bytes,
                        downloaded_at,
                    });
                }
            }
        }

        Ok(models)
    }

    async fn is_model_downloaded(&self, model_id: &str) -> Result<bool, AppError> {
        let paths = ModelPaths::new(model_id)?;

        // Check unified location first
        if self
            .validator
            .has_valid_model_files(paths.unified_path())
            .await
        {
            return Ok(true);
        }

        // Check legacy location
        if let Some(legacy_path) = paths.legacy_path() {
            if self.validator.has_valid_model_files(legacy_path).await {
                // Attempt migration
                debug!(
                    model_id = %model_id,
                    "Found model in legacy location, attempting migration"
                );

                if let Err(e) =
                    self.migrate_from_legacy(model_id, legacy_path, paths.unified_path())
                {
                    warn!(
                        model_id = %model_id,
                        error = %e,
                        "Failed to migrate model from legacy location"
                    );
                    return Ok(true);
                }

                return Ok(true);
            }
        }

        Ok(false)
    }

    async fn get_model_path(&self, model_id: &str) -> Result<PathBuf, AppError> {
        let paths = ModelPaths::new(model_id)?;

        // Check unified location first
        if self
            .validator
            .has_valid_model_files(paths.unified_path())
            .await
        {
            return Ok(paths.unified_path().to_path_buf());
        }

        // Check legacy location and migrate if found
        if let Some(legacy_path) = paths.legacy_path() {
            if self.validator.has_valid_model_files(legacy_path).await {
                self.migrate_from_legacy(model_id, legacy_path, paths.unified_path())?;
                return Ok(paths.unified_path().to_path_buf());
            }
        }

        Err(AppError::NotFound(format!("Model not found: {}", model_id)))
    }

    async fn delete_model(&self, model_id: &str) -> Result<(), AppError> {
        let paths = ModelPaths::new(model_id)?;

        // Try to delete from unified location
        if paths.unified_path().exists() {
            std::fs::remove_dir_all(paths.unified_path())?;
            return Ok(());
        }

        // Try to delete from legacy location
        if let Some(legacy_path) = paths.legacy_path() {
            if legacy_path.exists() {
                std::fs::remove_dir_all(legacy_path)?;
                return Ok(());
            }
        }

        Err(AppError::NotFound(format!("Model not found: {}", model_id)))
    }
}
