//! Deletes a downloaded model record and optionally the file.

use crate::audit::AuditAction;
use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::shared::error::{AppError, Result};
use std::path::Path;
use std::time::Duration;
use tokio::fs;
use tokio::time::sleep;
use tracing::{error, info, warn};

/// Maximum retry attempts for concurrent modification conflicts
const MAX_RETRY_ATTEMPTS: u32 = 3;

/// Base delay between retries (exponential backoff)
const RETRY_BASE_DELAY_MS: u64 = 10;

/// Validates that a path is safe for directory deletion.
fn is_safe_model_directory(path: &Path, model_id: &str) -> Result<bool> {
    // Gate 1: Must be under the models directory
    let path_str = path.to_str().ok_or_else(|| {
        AppError::FileSystem(format!("Invalid path encoding: {}", path.display()))
    })?;

    if !path_str.contains("/models/") && !path_str.contains("\\models\\") {
        return Err(AppError::FileSystem(format!(
            "Unsafe deletion: Path does not contain models directory: {}",
            path.display()
        )));
    }

    // Gate 2: Must be at least one level deeper than models_dir
    let components: Vec<_> = path.components().collect();
    let models_idx = components.iter().position(|c| {
        if let std::path::Component::Normal(os_str) = c {
            *os_str == std::ffi::OsStr::new("models")
        } else {
            false
        }
    });

    if let Some(idx) = models_idx {
        if idx + 1 >= components.len() {
            return Err(AppError::FileSystem(format!(
                "Unsafe deletion: Path is models root directory: {}",
                path.display()
            )));
        }
    } else {
        return Err(AppError::FileSystem(format!(
            "Unsafe deletion: Cannot find models directory in path: {}",
            path.display()
        )));
    }

    // Gate 3: Directory name must match model_id or parent must be "models"/"custom"
    let dir_name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| {
        AppError::FileSystem(format!(
            "Unsafe deletion: Cannot extract directory name from path: {}",
            path.display()
        ))
    })?;

    let parent_name = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str());

    let is_valid_structure = dir_name.contains(model_id)
        || parent_name == Some("models")
        || parent_name == Some("custom");

    if !is_valid_structure {
        return Err(AppError::FileSystem(format!(
            "Unsafe deletion: Directory name '{}' does not match model_id '{}' and parent is not models/custom: {}",
            dir_name,
            model_id,
            path.display()
        )));
    }

    Ok(true)
}

/// Deletes a downloaded model record and, optionally, the backing file.
pub struct DeleteDownloadedModelUseCase {
    repository: DownloadedModelRepository,
}

impl DeleteDownloadedModelUseCase {
    pub fn new(repository: DownloadedModelRepository) -> Self {
        Self { repository }
    }

    /// Execute with retry on concurrent modification conflicts.
    pub async fn execute(&self, id: &str, delete_file: bool) -> Result<()> {
        for attempt in 0..MAX_RETRY_ATTEMPTS {
            if attempt > 0 {
                let delay_ms = RETRY_BASE_DELAY_MS * (1 << attempt);
                warn!(attempt, delay_ms, "Concurrent modification detected, retrying deletion");
                sleep(Duration::from_millis(delay_ms)).await;
            }

            match self.try_delete_atomic(id, delete_file).await {
                Ok(()) => return Ok(()),
                Err(AppError::ConcurrentModification { .. }) if attempt < MAX_RETRY_ATTEMPTS - 1 =>
                    continue,
                Err(e) => return Err(e),
            }
        }

        Err(AppError::ConcurrentModification {
            resource: format!("model with id {}", id),
            details: "Max retry attempts exceeded".to_string(),
        })
    }

    /// Try to delete the model atomically (single attempt).
    async fn try_delete_atomic(&self, id: &str, delete_file: bool) -> Result<()> {
        let maybe_deleted_model = self.repository.delete_if_not_active(id).await?;

        match maybe_deleted_model {
            Some(deleted_model) => {
                info!(
                    id = %id,
                    model_id = %deleted_model.model_id(),
                    model_name = %deleted_model.model_name(),
                    "Model record deleted from database (atomic)"
                );

                if delete_file {
                    let is_external_model = deleted_model
                        .metadata()
                        .and_then(|meta| meta.get("source"))
                        .and_then(|value| value.as_str())
                        .map(|source| source == "external_directory")
                        .unwrap_or(false)
                        || deleted_model
                            .metadata()
                            .and_then(|meta| meta.get("external"))
                            .and_then(|value| value.as_bool())
                            .unwrap_or(false);

                    if is_external_model {
                        info!(model_id = %deleted_model.model_id(), "Skipping file deletion for external model record");
                        return Ok(());
                    }

                    let Some(file_path) = deleted_model.loadable_path() else {
                        info!(model_id = %deleted_model.model_id(), "Skipping file deletion for remote-hosted model");
                        return Ok(());
                    };

                    if file_path.exists() {
                        let model_dir = deleted_model
                            .location()
                            .enclosing_dir()
                            .ok_or_else(|| {
                                AppError::FileSystem(format!(
                                    "Cannot determine enclosing directory for: {}",
                                    file_path.display()
                                ))
                            })?;
                        let model_dir = model_dir.as_path();

                        match is_safe_model_directory(model_dir, deleted_model.model_id()) {
                            Ok(true) => {
                                // Safe to delete entire directory
                                match fs::remove_dir_all(model_dir).await {
                                    Ok(_) => {
                                        info!(directory = %model_dir.display(), model_id = %deleted_model.model_id(),
                                              "Model directory deleted successfully");
                                    }
                                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                                        warn!(directory = %model_dir.display(), "Model directory already deleted");
                                    }
                                    #[cfg(target_os = "windows")]
                                    Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                                        error!(error = %e, directory = %model_dir.display(),
                                               "Failed to delete model directory: File in use (Windows file locking)");
                                    }
                                    Err(e) => {
                                        error!(error = %e, directory = %model_dir.display(),
                                               "Failed to delete model directory");
                                    }
                                }
                            }
                            Ok(false) => {
                                // Safety validation failed — fall back to single file deletion
                                warn!(
                                    directory = %model_dir.display(),
                                    model_id = %deleted_model.model_id(),
                                    "Falling back to single-file deletion (safety check failed)"
                                );
                                match fs::remove_file(&file_path).await {
                                    Ok(_) => {
                                        info!(filepath = %file_path.display(), model_id = %deleted_model.model_id(),
                                              "Model file deleted successfully");
                                    }
                                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                                        warn!(filepath = %file_path.display(), "Model file already deleted");
                                    }
                                    #[cfg(target_os = "windows")]
                                    Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                                        error!(error = %e, filepath = %file_path.display(),
                                               "Failed to delete model file: Access denied (file in use)");
                                    }
                                    Err(e) => {
                                        error!(error = %e, filepath = %file_path.display(),
                                               "Failed to delete model file");
                                    }
                                }
                            }
                            Err(e) => {
                                // Safety validation returned an error — fall back to single file deletion
                                warn!(
                                    directory = %model_dir.display(),
                                    error = %e,
                                    model_id = %deleted_model.model_id(),
                                    "Falling back to single-file deletion (safety error)"
                                );
                                match fs::remove_file(&file_path).await {
                                    Ok(_) => {
                                        info!(filepath = %file_path.display(), model_id = %deleted_model.model_id(),
                                              "Model file deleted successfully");
                                    }
                                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                                        warn!(filepath = %file_path.display(), "Model file already deleted");
                                    }
                                    #[cfg(target_os = "windows")]
                                    Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                                        error!(error = %e, filepath = %file_path.display(),
                                               "Failed to delete model file: Access denied (file in use)");
                                    }
                                    Err(e) => {
                                        error!(error = %e, filepath = %file_path.display(),
                                               "Failed to delete model file");
                                    }
                                }
                            }
                        };
                    } else {
                        info!(filepath = %file_path.display(), "Model file does not exist, skipping deletion");
                    }


                }

                return Ok(());
            }
            None => {
                return Err(AppError::NotFound(format!("Model with id {} is active or does not exist", id)));
            }
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
mod tests {
    use super::*;
    use crate::app_state::AppStorage;
    use crate::domain::downloaded_model::ModelLocation;
    use crate::domain::file::model::FileType;
    use crate::infrastructure::persistence::repositories::{
        FileRepository, MetadataRepository,
    };
    use chrono::Utc;
    use serde_json::json;
    use serial_test::serial;
    use std::io::Write;
    use tempfile::tempdir;

    async fn setup_test_state() -> (AppStorage, std::path::PathBuf) {
        let temp_dir = tempdir().expect("create temp dir");
        let models_dir = temp_dir.path().to_path_buf();

        (
            AppStorage::new(
                models_dir.clone(),
                true,
                None,
                None,
                None,
            )
            .await
            .expect("create app storage"),
            models_dir,
        )
    }

    #[tokio::test]
    #[serial]
    async fn deletes_file_locally_and_removes_record_without_active_lock() {
        let (app_storage, _models_dir) = setup_test_state().await;

        let file_repo = FileRepository::new(app_storage.clone());
        let repo = app_storage.downloaded_model_repository();

        let model_info = serde_json::from_value(json!({
            "id": "test_model_001",
            "model_name": "Test Model",
            "model_size_bytes": 12345,
            "file_path": "/test/models/test_model.gguf",
            "file_type": "GGUF",
            "source": "model_directory",
            "status": "downloaded",
            "download_progress": 100.0,
            "last_modified": "2023-01-01T00:00:00Z",
            "created_at": "2023-01-01T00:00:00Z",
            "updated_at": "2023-01-01T00:00:00Z"
        }))
        .expect("deserialize model info");

        repo.insert_or_update_model_record(&model_info).await.expect("insert model");

        // Create the physical file so deletion has something to remove
        let file_path = Path::new("/test/models/test_model.gguf").parent().unwrap().to_path_buf();
        let file_path = file_path.join("test_model.gguf");
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await.expect("create parent dirs");
        }
        let mut file = fs::File::create(&file_path).await.expect("create file");
        file.write_all(b"test data").await.expect("write file");

        let use_case = DeleteDownloadedModelUseCase::new(repo.clone());
        use_case.execute(&model_info.id, true).await.expect("delete model");

        let result = repo.find_by_model_id(&model_info.id).await.expect("query model");
        assert!(result.is_none(), "model should be deleted");

        assert!(!file_path.exists(), "file should be deleted");
    }

    #[tokio::test]
    #[serial]
    async fn rejects_deletion_of_active_embedding_model() {
        let (app_storage, _models_dir) = setup_test_state().await;

        let repo = app_storage.downloaded_model_repository();

        let model_info = serde_json::from_value(json!({
            "id": "active_embedding_001",
            "model_name": "Active Embedding Model",
            "model_size_bytes": 1024,
            "file_path": "/test/models/embedding.gguf",
            "file_type": "GGUF",
            "source": "model_directory",
            "status": "downloaded",
            "download_progress": 100.0,
            "last_modified": "2023-01-01T00:00:00Z",
            "created_at": "2023-01-01T00:00:00Z",
            "updated_at": "2023-01-01T00:00:00Z"
        }))
        .expect("deserialize model info");

        repo.insert_or_update_model_record(&model_info).await.expect("insert model");
        repo.set_active_embedding_model(&model_info.model_name)
            .await
            .expect("set active embedding");

        let use_case = DeleteDownloadedModelUseCase::new(repo.clone());
        let result = use_case.execute(&model_info.id, true).await;

        // Atomic delete-if-not-active should have failed; model record remains.
        assert!(result.is_err(), "should reject deletion of active embedding model");
        assert!(
            repo.find_by_model_id(&model_info.id).await.expect("query model").is_some(),
            "model should still exist"
        );
    }

    #[tokio::test]
    #[serial]
    async fn cleans_up_empty_directories_after_deletion() {
        let (app_storage, _models_dir) = setup_test_state().await;
        let repo = app_storage.downloaded_model_repository();

        // Seed two models in the same directory so that after deleting one, the other remains
        // and the directory is NOT removed.
        for name in &["shared_dir_model_a", "shared_dir_model_b"] {
            let info = serde_json::from_value(json!({
                "id": name,
                "model_name": name,
                "model_size_bytes": 100,
                "file_path": format!("/test/models/shared/{}.gguf", name),
                "file_type": "GGUF",
                "source": "model_directory",
                "status": "downloaded",
                "download_progress": 100.0,
                "last_modified": "2023-01-01T00:00:00Z",
                "created_at": "2023-01-01T00:00:00Z",
                "updated_at": "2023-01-01T00:00:00Z"
            }))
            .expect("deserialize");

            repo.insert_or_update_model_record(&info).await.expect("insert");

            let dir = format!("/test/models/shared");
            fs::create_dir_all(&dir).await.expect("create dir");
            let file = format!("{}/{}.gguf", dir, name);
            let mut f = fs::File::create(&file).await.expect("create file");
            f.write_all(b"test").await.expect("write");
        }

        // Delete only one of them
        let use_case = DeleteDownloadedModelUseCase::new(repo.clone());
        use_case
            .execute("shared_dir_model_a", true)
            .await
            .expect("delete model a");

        // Model B should still exist, so directory should remain
        assert!(
            repo.find_by_model_id("shared_dir_model_b")
                .await
                .expect("query b")
                .is_some(),
            "model b should remain"
        );
        assert!(
            Path::new("/test/models/shared").exists(),
            "shared directory should still exist"
        );
    }
}
