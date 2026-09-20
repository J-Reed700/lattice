use crate::domain::download::{DownloadError, DownloadState};
use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::infrastructure::persistence::DownloadRepository;
use chrono::{DateTime, Utc};
use std::path::Path;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Reconcile stale download sessions on app startup
///
/// Finds downloads stuck in "Downloading" state (from app crash)
/// and marks them as Failed to prevent zombie states.
///
/// # Returns
/// Number of sessions reconciled
pub async fn reconcile_stale_downloads(
    repository: Arc<dyn DownloadRepository>,
    startup_ts: DateTime<Utc>,
) -> Result<usize, DownloadError> {
    info!("Starting download session reconciliation");

    let downloading_sessions = repository
        .list_by_state(&DownloadState::Downloading)
        .await?;

    // No task from a previous process survives a restart, so a session still
    // marked Downloading that was last touched before this one started is dead
    // however recently that was. A session this process has started is touched
    // after `startup_ts` and is left alone. An earlier thirty-second allowance
    // here let a quick relaunch keep its dead session as a row that never moved.
    let stale_sessions: Vec<_> = downloading_sessions
        .into_iter()
        .filter(|session| session.is_stale_at(startup_ts))
        .collect();
    let count = stale_sessions.len();

    if count == 0 {
        info!("No stale downloads found");
        return Ok(0);
    }

    warn!("Found {} stale download(s) in 'Downloading' state", count);

    let mut reconciled = 0;
    for mut session in stale_sessions {
        let session_id = session.id().to_string();

        match session.fail("App restarted while downloading".to_string()) {
            Ok(_) => match repository.update(&session).await {
                Ok(_) => {
                    info!("Reconciled stale download: {}", session_id);
                    reconciled += 1;
                }
                Err(e) => {
                    error!("Failed to update reconciled session {}: {}", session_id, e);
                }
            },
            Err(e) => {
                error!("Failed to mark session {} as failed: {}", session_id, e);
            }
        }
    }

    info!("Reconciled {} of {} stale downloads", reconciled, count);
    Ok(reconciled)
}

/// Reconcile orphaned download sessions on app startup
///
/// Finds completed downloads that have no corresponding model record.
/// Deletes the session to unblock the user from retrying.
///
/// The database record is removed even if file cleanup fails.
///
/// # Returns
/// Number of orphaned sessions cleaned
pub async fn reconcile_orphaned_sessions(
    download_repo: Arc<dyn DownloadRepository>,
    model_repo: Arc<DownloadedModelRepository>,
) -> Result<usize, DownloadError> {
    info!("Checking for orphaned download sessions");

    let completed_sessions = download_repo
        .list_by_state(&DownloadState::Completed)
        .await?;

    let mut cleaned = 0;

    for session in completed_sessions {
        let model_id = session.model_id();
        let session_id = session.id().to_string();

        let model_exists = match model_id {
            // A row that cannot be read is still a row: a model whose download
            // has not finished has no storage path yet and fails to map. Its
            // sessions are not orphans, and one such row must not end the pass
            // for every other session, which is what returning the error did.
            Some(id) => match model_repo.find_by_model_id(id).await {
                Ok(found) => found.is_some(),
                Err(e) => {
                    warn!(
                        "Could not read the model row for {} while checking session {}; \
                         leaving the session alone: {}",
                        id, session_id, e
                    );
                    true
                }
            },
            None => false, // No model_id means it's orphaned
        };

        if !model_exists {
            warn!(
                "Found orphaned session: {} (model_id: {})",
                session_id,
                model_id.unwrap_or("none")
            );

            // CRITICAL: Database First Strategy
            // Delete session FIRST to unblock user (even if file delete fails)
            match download_repo.delete(session.id()).await {
                Ok(_) => {
                    info!("Deleted orphaned session: {}", session_id);
                    cleaned += 1;

                    // Best-effort file cleanup (failures are logged but don't block)
                    let file_path = session.destination();
                    match std::fs::remove_file(file_path) {
                        Ok(_) => info!("Cleaned up file: {:?}", file_path),
                        Err(e) => warn!(
                            "File cleanup failed (non-critical): {:?} - {}",
                            file_path, e
                        ),
                    }
                }
                Err(e) => error!("Failed to delete orphaned session {}: {}", session_id, e),
            }
        }
    }

    if cleaned > 0 {
        info!("Cleaned up {} orphaned session(s)", cleaned);
    } else {
        info!("No orphaned sessions found");
    }

    Ok(cleaned)
}

/// Reconcile orphaned model files on app startup.
///
/// Scans the models directory for files that have no corresponding database record.
/// Deletes orphaned files to free up disk space.
///
/// Only files without a database record are deleted.
///
/// # Arguments
///
/// * `model_repo` - Repository to check if files have corresponding models
/// * `models_dir` - Path to the models directory (e.g., ~/.cache/lattice/models)
///
/// # Returns
/// Number of orphaned files cleaned
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
    use crate::domain::download::DownloadSession;
    use crate::features::download::download_repository::mock::MockDownloadRepository;

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

    fn downloading(id: &str) -> DownloadSession {
        let mut session = DownloadSession::new(
            id.to_string(),
            format!("https://example.com/{id}"),
            std::env::temp_dir().join(id),
            Some(1000),
            None,
        )
        .unwrap();
        session.start().unwrap();
        session
    }

    /// The case that shipped broken: the app is closed mid-download and opened
    /// again within seconds. The session was touched a moment before startup,
    /// which an earlier thirty-second allowance read as "still alive".
    #[tokio::test]
    async fn a_session_interrupted_moments_before_a_relaunch_is_closed_out() {
        let repository = Arc::new(MockDownloadRepository::new());
        repository
            .create(&downloading("interrupted"))
            .await
            .unwrap();
        let startup = Utc::now() + chrono::Duration::milliseconds(5);

        let reconciled = reconcile_stale_downloads(repository.clone(), startup)
            .await
            .unwrap();

        assert_eq!(reconciled, 1);
        let session = repository.get("interrupted").await.unwrap().unwrap();
        assert_eq!(session.state(), &DownloadState::Failed);
    }

    #[tokio::test]
    async fn a_download_started_by_this_launch_is_left_running() {
        let repository = Arc::new(MockDownloadRepository::new());
        let startup = Utc::now() - chrono::Duration::milliseconds(5);
        repository.create(&downloading("live")).await.unwrap();

        let reconciled = reconcile_stale_downloads(repository.clone(), startup)
            .await
            .unwrap();

        assert_eq!(reconciled, 0);
        let session = repository.get("live").await.unwrap().unwrap();
        assert_eq!(session.state(), &DownloadState::Downloading);
    }
}
