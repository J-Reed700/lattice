//! Model directories on disk that no model record owns.

use crate::infrastructure::persistence::repositories::downloaded_models::DownloadedModelRepository;
use lattice_core::domain::download::DownloadError;
use lattice_core::shared::fs::SweepGate;
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

/// Count, on app startup, the model directories no model record names, and
/// keep them.
///
/// That no row names a directory is no evidence it is garbage: the model
/// manager keeps the reranker here with no row of its own, and a restored
/// backup or a migration can lose rows of models still on disk. Nothing this
/// app records names a directory here as let go of (a deleted model's own
/// delete removes its directory), so none is removed; freeing that space is a
/// user's decision. Returns how many were removed, which is none. Does nothing
/// while `gate` is shut.
pub async fn reconcile_orphaned_files(
    model_repo: Arc<DownloadedModelRepository>,
    models_dir: &Path,
    gate: &SweepGate,
) -> Result<usize, DownloadError> {
    if !gate.is_open() {
        info!("Left the model directories alone: this database does not own the library files");
        return Ok(0);
    }
    if !models_dir.exists() {
        return Ok(0);
    }

    let read_dir = std::fs::read_dir(models_dir)
        .map_err(|e| DownloadError::IoError(format!("Failed to read models directory: {}", e)))?;

    let mut unexplained = 0usize;
    for entry in read_dir {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                warn!("Failed to read directory entry: {}", e);
                continue;
            }
        };
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(model_id) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // A row that cannot be read is still a row.
        if let Ok(None) = model_repo.find_by_model_id(model_id).await {
            unexplained += 1;
        }
    }
    if unexplained > 0 {
        info!(
            unexplained,
            "{} model folders no record explains; kept", unexplained
        );
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::shared::persistence::Database;

    fn open_gate() -> SweepGate {
        let gate = SweepGate::default();
        gate.open();
        gate
    }

    #[tokio::test]
    async fn orphan_cleanup_keeps_every_directory_no_record_names() {
        let pool = lattice_core::shared::testing::sqlite::migrated_in_memory()
            .await
            .unwrap();
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
        let repo = Arc::new(DownloadedModelRepository::new(Database::new(pool.clone())));
        assert!(repo.find_by_model_id("pending").await.is_err());
        assert!(repo.find_by_model_id("installed").await.unwrap().is_some());

        assert_eq!(
            reconcile_orphaned_files(repo, dir.path(), &open_gate())
                .await
                .unwrap(),
            0
        );
        for id in ["pending", "installed", "orphan"] {
            assert_eq!(
                std::fs::read(dir.path().join(id).join("weights")).unwrap(),
                b"model data"
            );
        }
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
        let repo = Arc::new(DownloadedModelRepository::new(Database::new(pool.clone())));

        assert_eq!(
            reconcile_orphaned_files(repo, dir.path(), &open_gate())
                .await
                .unwrap(),
            0
        );
        assert_eq!(std::fs::read(model.join("weights")).unwrap(), b"keep");
    }

    #[tokio::test]
    async fn orphan_cleanup_keeps_every_model_while_the_gate_is_shut() {
        let pool = lattice_core::shared::testing::sqlite::migrated_in_memory()
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let orphan = dir.path().join("orphan");
        std::fs::create_dir(&orphan).unwrap();
        std::fs::write(orphan.join("weights"), b"keep").unwrap();
        let repo = Arc::new(DownloadedModelRepository::new(Database::new(pool.clone())));

        let removed = reconcile_orphaned_files(repo, dir.path(), &SweepGate::default())
            .await
            .unwrap();

        assert_eq!(removed, 0);
        assert_eq!(std::fs::read(orphan.join("weights")).unwrap(), b"keep");
    }
}
