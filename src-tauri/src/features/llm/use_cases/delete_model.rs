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
//! The database is the Single Source of Truth for "does this model
//! exist". Existence and size are sourced from the `models` table, NOT
//! from the filesystem.
//!
//! Filesystem deletion is then performed best-effort: missing files
//! (e.g. user removed them via Finder) are logged and ignored. The
//! authoritative deletion is the DB row removal, which is propagated
//! on failure. This prevents the orphan-on-FS-miss bug where a
//! manually-deleted file made the model un-deletable from the UI
//! because the FS check happened before the DB delete.
//!
//! # Dependencies
//! - `ModelStoragePort` - Filesystem model storage
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

use crate::application::ports::model_storage::ModelStoragePort;
use crate::domain::curated_models::get_all_curated_models;
use crate::domain::repositories::downloaded_model_repository::DownloadedModelRepository;
use crate::features::llm::dto::{DeleteModelRequestDto, DeleteModelResponseDto};
use crate::shared::error::AppError;
use std::sync::Arc;
use tracing::{info, warn};

pub struct DeleteModelUseCase {
    storage: Arc<dyn ModelStoragePort>,
    repository: Arc<dyn DownloadedModelRepository>,
}

impl DeleteModelUseCase {
    /// Create a new DeleteModelUseCase with both filesystem and database deletion
    ///
    /// # Arguments
    /// * `storage` - Filesystem storage port for deleting model files
    /// * `repository` - Database repository for deleting model records
    pub fn new(
        storage: Arc<dyn ModelStoragePort>,
        repository: Arc<dyn DownloadedModelRepository>,
    ) -> Self {
        Self {
            storage,
            repository,
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

        self.execute_db_ssot(model_id).await
    }

    /// DB-as-SSOT deletion path.
    ///
    /// 1. Source the model from the DB; NotFound iff the DB row is absent.
    /// 2. Delete files best-effort (NotFound errors logged and ignored).
    /// 3. Delete the DB row authoritatively (errors propagate).
    async fn execute_db_ssot(&self, model_id: &str) -> Result<DeleteModelResponseDto, AppError> {
        let repository = self.repository.as_ref();
        let downloaded = repository
            .find_by_model_id(model_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Model {} is not registered", model_id)))?;

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
                // Single-file model - delegate to storage, tolerating NotFound
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
    //! The use case is DB-as-SSOT: existence and freed size come from the
    //! `models` table, the filesystem is cleaned best-effort. Tests therefore
    //! seed an in-memory SQLite repository and use the storage mock only to
    //! observe filesystem-side effects.

    use super::*;
    use crate::application::ports::model_storage::{
        DownloadedModel as StoredModel, MockModelStoragePort,
    };
    use crate::domain::downloaded_model::{DownloadedModel, ModelLocation};
    use crate::infrastructure::persistence::repositories::DownloadedModelRepository as ConcreteRepo;
    use chrono::Utc;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::path::PathBuf;

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

    async fn register(repo: &ConcreteRepo, model_id: &str, size_bytes: i64) {
        repo.save(&make_local_model(model_id, size_bytes))
            .await
            .expect("seed model in DB");
    }

    fn use_case(storage: Arc<MockModelStoragePort>, repo: &ConcreteRepo) -> DeleteModelUseCase {
        let repository: Arc<dyn DownloadedModelRepository> = Arc::new(repo.clone());
        DeleteModelUseCase::new(storage, repository)
    }

    fn request(model_id: &str) -> DeleteModelRequestDto {
        DeleteModelRequestDto {
            model_id: model_id.to_string(),
        }
    }

    #[tokio::test]
    async fn test_delete_model_success() {
        let repo = setup_repo().await;
        register(&repo, "phi-3-mini", 1_800_000_000).await;

        let storage = Arc::new(MockModelStoragePort::with_models());
        let use_case = use_case(storage.clone(), &repo);

        let response = use_case
            .execute(request("phi-3-mini"))
            .await
            .expect("delete succeeds");

        assert!(response.success);
        assert_eq!(response.freed_space_gb, 1.8);
        assert!(!storage.is_model_downloaded("phi-3-mini").await.unwrap());
        assert!(repo
            .find_by_model_id("phi-3-mini")
            .await
            .expect("query DB")
            .is_none());
    }

    #[tokio::test]
    async fn test_delete_model_not_found() {
        let repo = setup_repo().await;
        let storage = Arc::new(MockModelStoragePort::new());
        let use_case = use_case(storage, &repo);

        let result = use_case.execute(request("nonexistent")).await;

        match result {
            Err(AppError::NotFound(msg)) => {
                assert!(msg.contains("not registered"));
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_delete_model_empty_id() {
        let repo = setup_repo().await;
        let storage = Arc::new(MockModelStoragePort::new());
        let use_case = use_case(storage, &repo);

        let result = use_case.execute(request("")).await;

        match result {
            Err(AppError::InvalidInput(msg)) => {
                assert!(msg.contains("cannot be empty"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_delete_model_calculates_freed_space() {
        let repo = setup_repo().await;
        register(&repo, "test-model", 4_100_000_000).await;

        let storage = Arc::new(MockModelStoragePort::new());
        storage.add_model(StoredModel {
            model_id: "test-model".to_string(),
            path: PathBuf::from("/models/test"),
            size_bytes: 4_100_000_000,
            downloaded_at: Utc::now(),
        });

        let use_case = use_case(storage, &repo);

        let response = use_case
            .execute(request("test-model"))
            .await
            .expect("delete succeeds");

        assert!(response.success);
        assert_eq!(response.freed_space_gb, 4.1);
    }

    #[tokio::test]
    async fn test_delete_model_removes_from_storage() {
        let repo = setup_repo().await;
        register(&repo, "mistral-7b", 4_100_000_000).await;

        let storage = Arc::new(MockModelStoragePort::with_models());
        let use_case = use_case(storage.clone(), &repo);
        assert_eq!(storage.model_count(), 2);

        use_case
            .execute(request("mistral-7b"))
            .await
            .expect("delete succeeds");

        assert_eq!(storage.model_count(), 1);
        assert!(!storage.is_model_downloaded("mistral-7b").await.unwrap());
    }

    #[tokio::test]
    async fn test_delete_model_large_model() {
        let repo = setup_repo().await;
        register(&repo, "large-model", 26_000_000_000).await;

        let storage = Arc::new(MockModelStoragePort::new());
        let use_case = use_case(storage, &repo);

        let response = use_case
            .execute(request("large-model"))
            .await
            .expect("delete succeeds");

        assert!(response.success);
        assert_eq!(response.freed_space_gb, 26.0);
    }

    #[tokio::test]
    async fn test_delete_model_small_model() {
        let repo = setup_repo().await;
        register(&repo, "tiny-model", 1_100_000_000).await;

        let storage = Arc::new(MockModelStoragePort::new());
        let use_case = use_case(storage, &repo);

        let response = use_case
            .execute(request("tiny-model"))
            .await
            .expect("delete succeeds");

        assert!(response.success);
        assert_eq!(response.freed_space_gb, 1.1);
    }

    #[tokio::test]
    async fn test_delete_model_multiple_deletions() {
        let repo = setup_repo().await;
        register(&repo, "phi-3-mini", 1_800_000_000).await;
        register(&repo, "mistral-7b", 4_100_000_000).await;

        let storage = Arc::new(MockModelStoragePort::with_models());
        let use_case = use_case(storage.clone(), &repo);

        use_case
            .execute(request("phi-3-mini"))
            .await
            .expect("first delete succeeds");
        use_case
            .execute(request("mistral-7b"))
            .await
            .expect("second delete succeeds");

        assert_eq!(storage.model_count(), 0);
    }

    #[tokio::test]
    async fn test_delete_model_cannot_delete_twice() {
        let repo = setup_repo().await;
        register(&repo, "phi-3-mini", 1_800_000_000).await;

        let storage = Arc::new(MockModelStoragePort::with_models());
        let use_case = use_case(storage, &repo);

        use_case
            .execute(request("phi-3-mini"))
            .await
            .expect("first delete succeeds");

        match use_case.execute(request("phi-3-mini")).await {
            Err(AppError::NotFound(_)) => {}
            _ => panic!("Expected NotFound error on second deletion"),
        }
    }

    /// Regression: the pre-fix code returned NotFound from
    /// `is_model_downloaded()` before touching the DB, so a model whose files
    /// the user removed via Finder stayed in the `models` table forever and
    /// could never be deleted from the UI.
    #[tokio::test]
    async fn test_delete_model_db_path_handles_missing_file_gracefully() {
        let repo = setup_repo().await;
        register(&repo, "phantom-model", 2_500_000_000).await;

        // Filesystem mock has NO file: delete_model returns NotFound.
        let storage = Arc::new(MockModelStoragePort::new());
        let use_case = use_case(storage, &repo);

        let response = use_case
            .execute(request("phantom-model"))
            .await
            .expect("DB-SSOT delete should succeed despite missing file");

        assert!(response.success);
        assert_eq!(response.freed_space_gb, 2.5);
        assert!(
            repo.find_by_model_id("phantom-model")
                .await
                .expect("query DB after delete")
                .is_none(),
            "DB row must be removed (authoritative delete)"
        );
    }
}
