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
//! # Repository Barrier Rule (DB-as-SSOT)
//!
//! When constructed via `new_with_repository`, the database is the
//! Single Source of Truth for "does this model exist". Existence and
//! size are sourced from the `models` table, NOT from the filesystem.
//!
//! Filesystem deletion is then performed best-effort: missing files
//! (e.g. user removed them via Finder) are logged and ignored. The
//! authoritative deletion is the DB row removal, which is propagated
//! on failure. This prevents the orphan-on-FS-miss bug where a
//! manually-deleted file made the model un-deletable from the UI
//! because the FS check happened before the DB delete.
//!
//! Legacy `new()` constructor retains the prior FS-first behavior for
//! backwards compatibility with existing tests.
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
use tracing::{debug, info, warn};

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
        // WHY: DB is SSOT for "does this model exist". We MUST source size and
        // existence from the DB row first, then delete files best-effort, then
        // delete the DB row authoritatively. Reversing this order (FS-first)
        // strands DB rows when files were removed out-of-band (e.g. via Finder).
        let model_id = &request.model_id;

        if model_id.is_empty() {
            return Err(AppError::InvalidInput("Model ID cannot be empty".into()));
        }

        if let Some(repository) = self.repository.clone() {
            self.execute_db_ssot(model_id, repository.as_ref()).await
        } else {
            debug!(
                "delete_model: using legacy FS-only path; consider migrating caller to new_with_repository"
            );
            self.execute_legacy(model_id).await
        }
    }

    /// DB-as-SSOT deletion path (preferred).
    ///
    /// 1. Source the model from the DB; NotFound iff the DB row is absent.
    /// 2. Delete files best-effort (NotFound errors logged and ignored).
    /// 3. Delete the DB row authoritatively (errors propagate).
    async fn execute_db_ssot(
        &self,
        model_id: &str,
        repository: &dyn DownloadedModelRepository,
    ) -> Result<DeleteModelResponseDto, AppError> {
        let downloaded = repository
            .find_by_model_id(model_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(format!("Model {} is not registered", model_id))
            })?;

        let size_bytes = downloaded.file_size_bytes();
        let freed_space_gb = size_bytes as f64 / 1_000_000_000.0;

        Self::delete_files_best_effort(self.storage.as_ref(), model_id).await?;

        repository.delete(model_id).await?;
        info!(
            model_id = %model_id,
            "Deleted model from database (DB-as-SSOT path)"
        );

        Ok(DeleteModelResponseDto {
            success: true,
            freed_space_gb,
        })
    }

    /// Legacy FS-first deletion path (backwards-compatible with `new()`).
    ///
    /// Preserved verbatim for existing tests built against `MockModelStoragePort`.
    async fn execute_legacy(
        &self,
        model_id: &str,
    ) -> Result<DeleteModelResponseDto, AppError> {
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
                // repository-barrier-allow: cleaning up the model's own directory after deletion, not a state query.
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

        // No repository configured (legacy mode / tests)
        warn!(
            model_id = %model_id,
            "DeleteModelUseCase running without database repository. \
             Database records will not be cleaned up."
        );

        Ok(DeleteModelResponseDto {
            success: true,
            freed_space_gb,
        })
    }

    /// Delete model files best-effort.
    ///
    /// - NotFound errors (file already removed out-of-band) are logged and ignored.
    /// - Other errors (Permission, etc.) propagate.
    /// - Path-traversal validation on filenames remains intact (CWE-22).
    /// - Empty-directory cleanup remains best-effort (errors propagate to surface
    ///   real I/O issues but a missing directory is tolerated upstream).
    async fn delete_files_best_effort(
        storage: &dyn ModelStoragePort,
        model_id: &str,
    ) -> Result<(), AppError> {
        let curated_models = get_all_curated_models();
        let curated_model = curated_models.iter().find(|m| m.id == *model_id);

        if let Some(model_meta) = curated_model {
            if !model_meta.files.is_empty() {
                info!(
                    model_id = %model_id,
                    file_count = model_meta.files.len(),
                    "Deleting multi-file model (best-effort FS)"
                );

                // Resolve model path; if missing on disk, skip FS deletion entirely.
                let model_path = match storage.get_model_path(model_id).await {
                    Ok(path) => path,
                    Err(AppError::NotFound(msg)) => {
                        warn!(
                            model_id = %model_id,
                            error = %msg,
                            "Model path not found on disk; skipping filesystem deletion"
                        );
                        return Ok(());
                    }
                    Err(e) => return Err(e),
                };

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

                // Now safe to delete files (best-effort: NotFound is logged, others propagate)
                for file in &model_meta.files {
                    let file_path = model_path.join(&file.filename);
                    match tokio::fs::remove_file(&file_path).await {
                        Ok(()) => {
                            info!(file = %file.filename, "Deleted model file");
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                            warn!(
                                file = %file.filename,
                                path = %file_path.display(),
                                "Model file already missing on disk; ignoring"
                            );
                        }
                        Err(e) => {
                            return Err(AppError::FileSystem(format!(
                                "Failed to delete {} at {}: {}",
                                file.filename,
                                file_path.display(),
                                e
                            )));
                        }
                    }
                }

                // Try to remove directory if empty (best-effort: missing dir is tolerated)
                // repository-barrier-allow: cleaning up the model's own directory after deletion, not a state query.
                match tokio::fs::read_dir(&model_path).await {
                    Ok(mut entries) => {
                        let next = entries.next_entry().await.map_err(|e| {
                            AppError::FileSystem(format!("Failed to check directory: {}", e))
                        })?;
                        if next.is_none() {
                            match tokio::fs::remove_dir(&model_path).await {
                                Ok(()) => info!("Removed empty model directory"),
                                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                                    warn!(
                                        path = %model_path.display(),
                                        "Model directory already removed; ignoring"
                                    );
                                }
                                Err(e) => {
                                    return Err(AppError::FileSystem(format!(
                                        "Failed to remove directory: {}",
                                        e
                                    )));
                                }
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        warn!(
                            path = %model_path.display(),
                            "Model directory already removed; ignoring"
                        );
                    }
                    Err(e) => {
                        return Err(AppError::FileSystem(format!(
                            "Failed to read directory: {}",
                            e
                        )));
                    }
                }
            } else {
                // Single-file model (or legacy) - delegate to storage, tolerating NotFound
                match storage.delete_model(model_id).await {
                    Ok(()) => {}
                    Err(AppError::NotFound(msg)) => {
                        warn!(
                            model_id = %model_id,
                            error = %msg,
                            "Storage reports model already missing; ignoring"
                        );
                    }
                    Err(e) => return Err(e),
                }
            }
        } else {
            // Not a curated model - delegate to storage, tolerating NotFound
            match storage.delete_model(model_id).await {
                Ok(()) => {}
                Err(AppError::NotFound(msg)) => {
                    warn!(
                        model_id = %model_id,
                        error = %msg,
                        "Storage reports model already missing; ignoring"
                    );
                }
                Err(e) => return Err(e),
            }
        }

        Ok(())
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

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
mod db_ssot_tests {
    //! Tests for the DB-as-SSOT path (constructor: `new_with_repository`).
    //!
    //! Regression coverage for the orphan-on-FS-miss bug: if the user removes
    //! the model file via Finder, the FS-first path stranded the DB row
    //! forever. The DB-SSOT path tolerates the missing file and authoritatively
    //! deletes the DB row.

    use super::*;
    use crate::application::ports::model_storage::MockModelStoragePort;
    use crate::domain::downloaded_model::{DownloadedModel, ModelLocation};
    use crate::infrastructure::persistence::repositories::DownloadedModelRepository as ConcreteRepo;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_repo() -> ConcreteRepo {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("create in-memory pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("run migrations");
        ConcreteRepo::new(pool)
    }

    fn make_local_model(model_id: &str, size_bytes: i64) -> DownloadedModel {
        DownloadedModel::new(
            uuid::Uuid::new_v4().to_string(),
            format!("Test Model {}", model_id),
            model_id.to_string(),
            ModelLocation::LocalFile {
                path: std::path::PathBuf::from(format!("/tmp/{}.gguf", model_id)),
            },
            size_bytes,
            "llama".to_string(),
            None,
        )
        .expect("create local model")
    }

    #[tokio::test]
    async fn test_delete_model_db_path_handles_missing_file_gracefully() {
        // Arrange:
        // - Model is registered in the DB (size 2_500_000_000 bytes).
        // - Filesystem mock has NO file (delete_model returns NotFound).
        // The pre-fix code returned NotFound from is_model_downloaded() before
        // touching the DB, leaving the row orphaned forever.
        let repo = setup_repo().await;
        let model_id = "phantom-model";
        let size_bytes: i64 = 2_500_000_000;
        let model = make_local_model(model_id, size_bytes);
        repo.save(&model).await.expect("seed model in DB");

        let storage = Arc::new(MockModelStoragePort::new());
        let repo_arc: Arc<dyn DownloadedModelRepository> = Arc::new(repo.clone());
        let use_case = DeleteModelUseCase::new_with_repository(storage.clone(), repo_arc);

        let request = DeleteModelRequestDto {
            model_id: model_id.to_string(),
        };

        // Act
        let result = use_case.execute(request).await;

        // Assert: success
        let response = result.expect("DB-SSOT delete should succeed despite missing file");
        assert!(response.success);
        assert_eq!(response.freed_space_gb, 2.5);

        // Assert: DB row gone
        let row = repo
            .find_by_model_id(model_id)
            .await
            .expect("query DB after delete");
        assert!(row.is_none(), "DB row must be removed (authoritative delete)");
    }
}
