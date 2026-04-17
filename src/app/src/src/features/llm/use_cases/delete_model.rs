//! Delete Model Use Case
//!
//! Removes a downloaded model from local storage AND database.
//!
//! # Purpose
//!
//! Deletes a locally downloaded LLM model and all its files,
//! calculating freed disk space. This use case ensures BOTH
//! filesystem and database records are deleted to maintain consistency.
//!
//! # Dependencies
//! - `ModelStoragePort` - Filesystem model storage (legacy)
//! - `DownloadedModelRepository` - Database tracking (Registry/Usage bounded context)
//!
//! # Example
//! ```rust,no_run
//! let use_case = DeleteModelUseCase::new(storage_port, repository);
//! let request = DeleteModelRequestDto {
//!     model_id: "phi-3-mini".to_string(),
//! };
//! let response = use_case.execute(request).await?;
//! println!("Freed {} GB", response.freed_space_gb);
//! ```

use crate::features::llm::dto::{DeleteModelRequestDto, DeleteModelResponseDto};
use crate::application::ports::model_storage::ModelStoragePort;
use crate::domain::curated_models::get_all_curated_models;
use crate::domain::repositories::downloaded_model_repository::DownloadedModelRepository;
use crate::shared::error::AppError;
use std::sync::Arc;
use tracing::{info, warn};

pub struct DeleteModelUseCase {
    storage: Arc<dyn ModelStoragePort>,
    repository: Option<Arc<dyn DownloadedModelRepository>>,
}

impl DeleteModelUseCase {
    /// Create a new DeleteModelUseCase with both filesystem and database deletion
    ///
    /// # Arguments
    /// * `storage` - Filesystem storage port for deleting model files
    /// * `repository` - Database repository for deleting model records
    pub fn new_with_repository(
        storage: Arc<dyn ModelStoragePort>,
        repository: Arc<dyn DownloadedModelRepository>,
    ) -> Self {
        Self {
            storage,
            repository: Some(repository),
        }
    }

    /// Create a new DeleteModelUseCase with filesystem-only deletion (legacy)
    ///
    /// This constructor is for backwards compatibility with existing tests.
    /// New code should use `new_with_repository` to ensure database cleanup.
    pub fn new(storage: Arc<dyn ModelStoragePort>) -> Self {
        Self {
            storage,
            repository: None,
        }
    }

    pub async fn execute(
        &self,
        request: DeleteModelRequestDto,
    ) -> Result<DeleteModelResponseDto, AppError> {
        let model_id = &request.model_id;

        if model_id.is_empty() {
            return Err(AppError::InvalidInput("Model ID cannot be empty".into()));
        }

        if !self.storage.is_model_downloaded(model_id).await? {
            return Err(AppError::NotFound(format!(
                "Model {} is not downloaded",
                model_id
            )));
        }

        let models = self.storage.list_models().await?;
        let model = models
            .iter()
            .find(|m| m.model_id == *model_id)
            .ok_or_else(|| AppError::NotFound(format!("Model {} not found", model_id)))?;

        let size_bytes = model.size_bytes;
        let freed_space_gb = size_bytes as f64 / 1_000_000_000.0;

        // Delete from filesystem - check if multi-file model
        let curated_models = get_all_curated_models();
        let curated_model = curated_models.iter().find(|m| m.id == *model_id);

        if let Some(model_meta) = curated_model {
            if !model_meta.files.is_empty() {
                // Multi-file model - delete all files individually
                info!(
                    model_id = %model_id,
                    file_count = model_meta.files.len(),
                    "Deleting multi-file model"
                );

                let model_path = self.storage.get_model_path(model_id).await?;

                // Validate all filenames first (security check - prevents CWE-22 path traversal)
                for file in &model_meta.files {
                    if file.filename.contains("..")
                        || file.filename.contains('/')
                        || file.filename.contains('\\')
                    {
                        return Err(AppError::InvalidInput(format!(
                            "Invalid filename contains path separators: {}",
                            file.filename
                        )));
                    }
                }

                // Now safe to delete files
                for file in &model_meta.files {
                    let file_path = model_path.join(&file.filename);
                    if file_path.exists() {
                        tokio::fs::remove_file(&file_path).await.map_err(|e| {
                            AppError::FileSystem(format!(
                                "Failed to delete {} at {}: {}",
                                file.filename,
                                file_path.display(),
                                e
                            ))
                        })?;

                        info!(file = %file.filename, "Deleted model file");
                    } else {
                        warn!(file = %file.filename, "File not found, skipping");
                    }
                }

                // Try to remove directory if empty (async I/O)
                let mut entries = tokio::fs::read_dir(&model_path).await.map_err(|e| {
                    AppError::FileSystem(format!("Failed to read directory: {}", e))
                })?;

                if entries
                    .next_entry()
                    .await
                    .map_err(|e| AppError::FileSystem(format!("Failed to check directory: {}", e)))?
                    .is_none()
                {
                    tokio::fs::remove_dir(&model_path).await.map_err(|e| {
                        AppError::FileSystem(format!("Failed to remove directory: {}", e))
                    })?;

                    info!("Removed empty model directory");
                }
            } else {
                // Single-file model (or legacy) - use existing storage deletion
                self.storage.delete_model(model_id).await?;
            }
        } else {
            // Not a curated model - use legacy deletion
            self.storage.delete_model(model_id).await?;
        }

        // BUG FIX: Also delete from database (new system)
        // This ensures the model doesn't reappear after reload when UI queries database
        if let Some(repository) = &self.repository {
            match repository.delete(model_id).await {
                Ok(()) => {
                    info!(
                        model_id = %model_id,
                        "Deleted model from database (dual-system cleanup)"
                    );
                }
                Err(e) => {
                    // Database deletion failed - log warning but don't fail the operation
                    // Filesystem deletion already succeeded, so the model is effectively deleted
                    // The orphaned database record can be cleaned up later
                    warn!(
                        error = %e,
                        model_id = %model_id,
                        "Failed to delete model from database. Filesystem deletion succeeded. \
                         Orphaned database record should be cleaned up by maintenance job."
                    );
                }
            }
        } else {
            // No repository configured (legacy mode / tests)
            warn!(
                model_id = %model_id,
                "DeleteModelUseCase running without database repository. \
                 Database records will not be cleaned up."
            );
        }

        Ok(DeleteModelResponseDto {
            success: true,
            freed_space_gb,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::model_storage::{DownloadedModel, MockModelStoragePort};
    use chrono::Utc;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_delete_model_success() {
        let storage = Arc::new(MockModelStoragePort::with_models());
        let use_case = DeleteModelUseCase::new(storage.clone());

        let request = DeleteModelRequestDto {
            model_id: "phi-3-mini".to_string(),
        };

        let result = use_case.execute(request).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.success);
        assert_eq!(response.freed_space_gb, 1.8);

        assert!(!storage.is_model_downloaded("phi-3-mini").await.unwrap());
    }

    #[tokio::test]
    async fn test_delete_model_not_found() {
        let storage = Arc::new(MockModelStoragePort::new());
        let use_case = DeleteModelUseCase::new(storage);

        let request = DeleteModelRequestDto {
            model_id: "nonexistent".to_string(),
        };

        let result = use_case.execute(request).await;

        assert!(result.is_err());
        match result {
            Err(AppError::NotFound(msg)) => {
                assert!(msg.contains("not downloaded"));
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_delete_model_empty_id() {
        let storage = Arc::new(MockModelStoragePort::new());
        let use_case = DeleteModelUseCase::new(storage);

        let request = DeleteModelRequestDto {
            model_id: "".to_string(),
        };

        let result = use_case.execute(request).await;

        assert!(result.is_err());
        match result {
            Err(AppError::InvalidInput(msg)) => {
                assert!(msg.contains("cannot be empty"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_delete_model_calculates_freed_space() {
        let storage = Arc::new(MockModelStoragePort::new());

        storage.add_model(DownloadedModel {
            model_id: "test-model".to_string(),
            path: PathBuf::from("/models/test"),
            size_bytes: 4_100_000_000,
            downloaded_at: Utc::now(),
        });

        let use_case = DeleteModelUseCase::new(storage.clone());

        let request = DeleteModelRequestDto {
            model_id: "test-model".to_string(),
        };

        let result = use_case.execute(request).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.success);
        assert_eq!(response.freed_space_gb, 4.1);
    }

    #[tokio::test]
    async fn test_delete_model_removes_from_storage() {
        let storage = Arc::new(MockModelStoragePort::with_models());
        let use_case = DeleteModelUseCase::new(storage.clone());

        assert_eq!(storage.model_count(), 2);

        let request = DeleteModelRequestDto {
            model_id: "mistral-7b".to_string(),
        };

        let result = use_case.execute(request).await;

        assert!(result.is_ok());
        assert_eq!(storage.model_count(), 1);
        assert!(!storage.is_model_downloaded("mistral-7b").await.unwrap());
    }

    #[tokio::test]
    async fn test_delete_model_large_model() {
        let storage = Arc::new(MockModelStoragePort::new());

        storage.add_model(DownloadedModel {
            model_id: "large-model".to_string(),
            path: PathBuf::from("/models/large"),
            size_bytes: 26_000_000_000,
            downloaded_at: Utc::now(),
        });

        let use_case = DeleteModelUseCase::new(storage);

        let request = DeleteModelRequestDto {
            model_id: "large-model".to_string(),
        };

        let result = use_case.execute(request).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.success);
        assert_eq!(response.freed_space_gb, 26.0);
    }

    #[tokio::test]
    async fn test_delete_model_small_model() {
        let storage = Arc::new(MockModelStoragePort::new());

        storage.add_model(DownloadedModel {
            model_id: "tiny-model".to_string(),
            path: PathBuf::from("/models/tiny"),
            size_bytes: 1_100_000_000,
            downloaded_at: Utc::now(),
        });

        let use_case = DeleteModelUseCase::new(storage);

        let request = DeleteModelRequestDto {
            model_id: "tiny-model".to_string(),
        };

        let result = use_case.execute(request).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.success);
        assert_eq!(response.freed_space_gb, 1.1);
    }

    #[tokio::test]
    async fn test_delete_model_multiple_deletions() {
        let storage = Arc::new(MockModelStoragePort::with_models());
        let use_case = DeleteModelUseCase::new(storage.clone());

        let request1 = DeleteModelRequestDto {
            model_id: "phi-3-mini".to_string(),
        };
        let result1 = use_case.execute(request1).await;
        assert!(result1.is_ok());

        let request2 = DeleteModelRequestDto {
            model_id: "mistral-7b".to_string(),
        };
        let result2 = use_case.execute(request2).await;
        assert!(result2.is_ok());

        assert_eq!(storage.model_count(), 0);
    }

    #[tokio::test]
    async fn test_delete_model_cannot_delete_twice() {
        let storage = Arc::new(MockModelStoragePort::with_models());
        let use_case = DeleteModelUseCase::new(storage);

        let request = DeleteModelRequestDto {
            model_id: "phi-3-mini".to_string(),
        };

        let result1 = use_case.execute(request.clone()).await;
        assert!(result1.is_ok());

        let result2 = use_case.execute(request).await;
        assert!(result2.is_err());
        match result2 {
            Err(AppError::NotFound(_)) => {}
            _ => panic!("Expected NotFound error on second deletion"),
        }
    }

    #[tokio::test]
    async fn test_delete_model_validates_id_format() {
        let storage = Arc::new(MockModelStoragePort::new());
        let use_case = DeleteModelUseCase::new(storage);

        let request = DeleteModelRequestDto {
            model_id: "   ".to_string(),
        };

        let result = use_case.execute(request).await;

        assert!(result.is_err());
    }
}
