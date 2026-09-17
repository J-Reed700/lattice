//! Deletes a downloaded model record and optionally the file.

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
                warn!(
                    attempt,
                    delay_ms, "Concurrent modification detected, retrying deletion"
                );
                sleep(Duration::from_millis(delay_ms)).await;
            }

            match self.try_delete_atomic(id, delete_file).await {
                Ok(()) => return Ok(()),
                Err(AppError::ConcurrentModification { .. })
                    if attempt < MAX_RETRY_ATTEMPTS - 1 =>
                {
                    continue
                }
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

                    // repository-barrier-allow: deletion verifies the model artifact before removing it.
                    if file_path.exists() {
                        let model_dir =
                            deleted_model.location().enclosing_dir().ok_or_else(|| {
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

                Ok(())
            }
            None => Err(AppError::NotFound(format!(
                "Model with id {} is active or does not exist",
                id
            ))),
        }
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
mod tests {
    use super::*;
    use crate::domain::downloaded_model::{DownloadedModel, ModelLocation};
    use serde_json::json;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::path::PathBuf;
    use tempfile::{tempdir, TempDir};

    async fn setup_repo() -> DownloadedModelRepository {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("create in-memory pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("run migrations");
        DownloadedModelRepository::new(pool)
    }

    /// Builds a realistic on-disk layout: `<tmp>/models/<model_id>/model.gguf`.
    /// The `models/` component matters — `is_safe_model_directory` refuses to
    /// recurse into anything that isn't under it.
    fn model_dir_with_weights(model_id: &str) -> (TempDir, PathBuf) {
        let temp = tempdir().expect("create temp dir");
        let dir = temp.path().join("models").join(model_id);
        std::fs::create_dir_all(&dir).expect("create model dir");
        let weights = dir.join("model.gguf");
        std::fs::write(&weights, b"test weights").expect("write weights");
        (temp, weights)
    }

    fn local_model(id: &str, model_id: &str, weights: &Path) -> DownloadedModel {
        DownloadedModel::new(
            id.to_string(),
            format!("Test {}", model_id),
            model_id.to_string(),
            ModelLocation::LocalFile {
                path: weights.to_path_buf(),
            },
            1024,
            "llama".to_string(),
            None,
        )
        .expect("construct local model")
    }

    #[test]
    fn safe_directory_accepts_model_dir_under_models() {
        let path = Path::new("/home/u/.cache/lattice/models/my-model");
        assert!(is_safe_model_directory(path, "my-model").expect("should validate"));
    }

    #[test]
    fn safe_directory_accepts_dir_whose_parent_is_models() {
        // Directory name need not contain the model id when the parent is `models`.
        let path = Path::new("/home/u/.cache/lattice/models/some-other-name");
        assert!(is_safe_model_directory(path, "my-model").expect("should validate"));
    }

    #[test]
    fn safe_directory_rejects_path_outside_models() {
        let path = Path::new("/home/u/Documents/important");
        assert!(is_safe_model_directory(path, "my-model").is_err());
    }

    #[test]
    fn safe_directory_rejects_models_root_itself() {
        let path = Path::new("/home/u/.cache/lattice/models");
        assert!(
            is_safe_model_directory(path, "my-model").is_err(),
            "deleting the models root would wipe every model"
        );
    }

    #[test]
    fn safe_directory_rejects_deep_mismatched_dir() {
        // Nested two levels below `models/`, name unrelated to the model id,
        // and parent is neither `models` nor `custom`.
        let path = Path::new("/home/u/.cache/lattice/models/vendor/unrelated");
        assert!(is_safe_model_directory(path, "my-model").is_err());
    }

    #[tokio::test]
    async fn deletes_record_and_file_for_inactive_local_model() {
        let repo = setup_repo().await;
        let (_temp, weights) = model_dir_with_weights("test-model-a");
        let model = local_model("row-a", "test-model-a", &weights);
        repo.save(&model).await.expect("save model");

        DeleteDownloadedModelUseCase::new(repo.clone())
            .execute("row-a", true)
            .await
            .expect("delete should succeed");

        assert!(
            repo.find_by_model_id("test-model-a")
                .await
                .expect("query")
                .is_none(),
            "record should be gone"
        );
        assert!(!weights.exists(), "weights should be removed from disk");
    }

    #[tokio::test]
    async fn keeps_file_when_delete_file_is_false() {
        let repo = setup_repo().await;
        let (_temp, weights) = model_dir_with_weights("test-model-b");
        let model = local_model("row-b", "test-model-b", &weights);
        repo.save(&model).await.expect("save model");

        DeleteDownloadedModelUseCase::new(repo.clone())
            .execute("row-b", false)
            .await
            .expect("delete should succeed");

        assert!(
            repo.find_by_model_id("test-model-b")
                .await
                .expect("query")
                .is_none(),
            "record should be gone"
        );
        assert!(
            weights.exists(),
            "weights should survive delete_file = false"
        );
    }

    #[tokio::test]
    async fn rejects_deletion_of_active_embedding_model() {
        let repo = setup_repo().await;
        let (_temp, weights) = model_dir_with_weights("test-model-c");
        let model = local_model("row-c", "test-model-c", &weights);
        repo.save(&model).await.expect("save model");
        let identity = crate::domain::value_objects::ArtifactIdentity::from_digest(&[0; 32]);
        repo.set_active_embedding_model("test-model-c", Some(&identity))
            .await
            .expect("activate for embedding");

        let result = DeleteDownloadedModelUseCase::new(repo.clone())
            .execute("row-c", true)
            .await;

        assert!(result.is_err(), "active model must not be deletable");
        assert!(
            repo.find_by_model_id("test-model-c")
                .await
                .expect("query")
                .is_some(),
            "record should survive a rejected delete"
        );
        assert!(weights.exists(), "weights must survive a rejected delete");
    }

    #[tokio::test]
    async fn external_model_record_is_removed_but_file_is_preserved() {
        let repo = setup_repo().await;
        let (_temp, weights) = model_dir_with_weights("test-model-d");
        let model = DownloadedModel::new(
            "row-d".to_string(),
            "External Model".to_string(),
            "test-model-d".to_string(),
            ModelLocation::LocalFile {
                path: weights.clone(),
            },
            1024,
            "llama".to_string(),
            Some(json!({ "source": "external_directory" })),
        )
        .expect("construct external model");
        repo.save(&model).await.expect("save model");

        DeleteDownloadedModelUseCase::new(repo.clone())
            .execute("row-d", true)
            .await
            .expect("delete should succeed");

        assert!(
            repo.find_by_model_id("test-model-d")
                .await
                .expect("query")
                .is_none(),
            "record should be gone"
        );
        assert!(
            weights.exists(),
            "a user's own file outside our control must never be deleted"
        );
    }

    #[tokio::test]
    async fn remote_model_deletion_touches_no_filesystem() {
        let repo = setup_repo().await;
        let model = DownloadedModel::new(
            "row-e".to_string(),
            "Ollama Model".to_string(),
            "test-model-e".to_string(),
            ModelLocation::RemoteOllama,
            0,
            "llama".to_string(),
            None,
        )
        .expect("construct remote model");
        repo.save(&model).await.expect("save model");

        DeleteDownloadedModelUseCase::new(repo.clone())
            .execute("row-e", true)
            .await
            .expect("remote delete should succeed");

        assert!(
            repo.find_by_model_id("test-model-e")
                .await
                .expect("query")
                .is_none(),
            "record should be gone"
        );
    }

    #[tokio::test]
    async fn deleting_unknown_id_reports_not_found() {
        let repo = setup_repo().await;
        let result = DeleteDownloadedModelUseCase::new(repo)
            .execute("no-such-row", true)
            .await;
        assert!(result.is_err(), "unknown id should error");
    }
}
