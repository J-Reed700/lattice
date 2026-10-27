//! Model directories on disk that no model record owns.

use crate::infrastructure::persistence::repositories::downloaded_models::DownloadedModelRepository;
use lattice_core::domain::download::DownloadError;
use std::path::Path;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Reconcile orphaned model files on app startup.
///
/// Scans the models directory for files that have no corresponding database record.
/// Deletes orphaned files to free up disk space.
///
/// Only files without a database record are deleted.
pub async fn reconcile_orphaned_files(
    model_repo: Arc<DownloadedModelRepository>,
    models_dir: &Path,
) -> Result<usize, DownloadError> {
    info!("Scanning for orphaned model files in: {:?}", models_dir);

    if !models_dir.exists() {
        info!("Models directory does not exist, skipping orphaned file cleanup");
        return Ok(0);
    }

    let mut cleaned = 0;
    let mut total_size_freed: u64 = 0;

    let read_dir = std::fs::read_dir(models_dir)
        .map_err(|e| DownloadError::IoError(format!("Failed to read models directory: {}", e)))?;

    for entry in read_dir {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                warn!("Failed to read directory entry: {}", e);
                continue;
            }
        };

        let model_dir_path = entry.path();

        // Skip if not a directory
        if !model_dir_path.is_dir() {
            continue;
        }

        let model_id = match model_dir_path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => {
                warn!("Invalid model directory name: {:?}", model_dir_path);
                continue;
            }
        };

        // An unreadable row must not end the pass, and must not read as
        // "no such model" either: that would delete a directory still in use.
        let model_exists = match model_repo.find_by_model_id(model_id).await {
            Ok(model) => model.is_some(),
            Err(e) => {
                warn!(
                    "Could not check model {} while looking for orphaned files; leaving {:?} alone: {}",
                    model_id, model_dir_path, e
                );
                continue;
            }
        };

        if !model_exists {
            // Orphaned model directory found
            warn!(
                "Found orphaned model directory: {:?} (model_id: {})",
                model_dir_path, model_id
            );

            // Calculate directory size before deletion
            if let Ok(size) = calculate_dir_size(&model_dir_path) {
                total_size_freed += size;
            }

            match std::fs::remove_dir_all(&model_dir_path) {
                Ok(_) => {
                    info!("Deleted orphaned model directory: {:?}", model_dir_path);
                    cleaned += 1;
                }
                Err(e) => {
                    error!(
                        "Failed to delete orphaned model directory {:?}: {}",
                        model_dir_path, e
                    );
                }
            }
        }
    }

    if cleaned > 0 {
        let size_mb = total_size_freed as f64 / 1_048_576.0; // Convert to MB
        info!(
            "Cleaned up {} orphaned model director(ies), freed {:.2} MB",
            cleaned, size_mb
        );
    } else {
        info!("No orphaned model files found");
    }

    Ok(cleaned)
}

/// Calculate total size of a directory recursively
fn calculate_dir_size(path: &Path) -> std::io::Result<u64> {
    let mut total_size = 0;

    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                total_size += std::fs::metadata(&path)?.len();
            } else if path.is_dir() {
                total_size += calculate_dir_size(&path)?;
            }
        }
    }

    Ok(total_size)
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn orphan_cleanup_preserves_unreadable_and_valid_models_but_removes_orphans() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        for id in ["pending", "installed", "orphan"] {
            std::fs::create_dir(dir.path().join(id)).unwrap();
            std::fs::write(dir.path().join(id).join("weights"), b"model data").unwrap();
        }
        // This is the real pending-download shape that the model mapper cannot
        // read: it has a row but no final storage path yet.
        sqlx::query("INSERT INTO models (id, model_id, model_name, base_path, total_size_bytes, status, model_type, architecture, storage_kind, storage_path) VALUES ('pending', 'pending', 'Pending', '', 10, 'downloading', 'chat', 'llama', 'local_file', NULL)")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO models (id, model_id, model_name, base_path, total_size_bytes, status, model_type, architecture, storage_kind, storage_path) VALUES ('installed', 'installed', 'Installed', '', 10, 'completed', 'chat', 'llama', 'local_dir', ?)")
            .bind(dir.path().join("installed").to_str().unwrap())
            .execute(&pool).await.unwrap();
        let repo = Arc::new(DownloadedModelRepository::new(pool));
        assert!(repo.find_by_model_id("pending").await.is_err());
        assert!(repo.find_by_model_id("installed").await.unwrap().is_some());

        assert_eq!(reconcile_orphaned_files(repo, dir.path()).await.unwrap(), 1);
        for id in ["pending", "installed"] {
            assert_eq!(
                std::fs::read(dir.path().join(id).join("weights")).unwrap(),
                b"model data"
            );
        }
        assert!(!dir.path().join("orphan").exists());
    }

    #[tokio::test]
    async fn orphan_cleanup_never_deletes_models_when_the_database_is_unavailable() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();
        pool.close().await;
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("model");
        std::fs::create_dir(&model).unwrap();
        std::fs::write(model.join("weights"), b"keep").unwrap();
        let repo = Arc::new(DownloadedModelRepository::new(pool));

        assert_eq!(reconcile_orphaned_files(repo, dir.path()).await.unwrap(), 0);
        assert_eq!(std::fs::read(model.join("weights")).unwrap(), b"keep");
    }
}
