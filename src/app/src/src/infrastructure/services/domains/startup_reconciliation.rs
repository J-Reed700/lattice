use crate::domain::download::{DownloadError, DownloadState};
use crate::persistence::repositories::DownloadedModelRepository;
use crate::persistence::DownloadRepository;
use chrono::{DateTime, Duration, Utc};
use std::path::{Path, PathBuf};
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
    grace: Duration,
) -> Result<usize, DownloadError> {
    info!("Starting download session reconciliation");

    let downloading_sessions = repository
        .list_by_state(&DownloadState::Downloading)
        .await?;

    let cutoff = startup_ts - grace;
    let stale_sessions: Vec<_> = downloading_sessions
        .into_iter()
        .filter(|session| session.is_stale_at(cutoff))
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
/// Oracle's "Database First" strategy: Delete session even if file cleanup fails.
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

        // Check if model exists
        let model_exists = match model_id {
            Some(id) => model_repo
                .find_by_model_id(id)
                .await
                .map_err(|e| {
                    DownloadError::IoError(format!("Failed to check model existence: {}", e))
                })?
                .is_some(),
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

/// Reconcile orphaned model files on app startup (Phase 3)
///
/// Scans the models directory for files that have no corresponding database record.
/// Deletes orphaned files to free up disk space.
///
/// Oracle's "Database First" strategy: Only delete files that have no DB record.
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

    // Check if models directory exists
    if !models_dir.exists() {
        info!("Models directory does not exist, skipping orphaned file cleanup");
        return Ok(0);
    }

    let mut cleaned = 0;
    let mut total_size_freed: u64 = 0;

    // Read all model directories
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

        // Extract model_id from directory name
        let model_id = match model_dir_path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => {
                warn!("Invalid model directory name: {:?}", model_dir_path);
                continue;
            }
        };

        // Check if model exists in database
        let model_exists = model_repo
            .find_by_model_id(model_id)
            .await
            .map_err(|e| DownloadError::IoError(format!("Failed to check model existence: {}", e)))?
            .is_some();

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

            // Delete the entire directory
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
