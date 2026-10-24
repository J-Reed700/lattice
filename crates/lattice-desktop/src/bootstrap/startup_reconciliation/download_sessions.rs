//! Download session rows left behind by the last run: dead downloads and orphaned sessions.

use crate::features::download::download_repository::DownloadRepository;
use crate::infrastructure::persistence::repositories::downloaded_models::DownloadedModelRepository;
use chrono::{DateTime, Utc};
use lattice_core::domain::download::{DownloadError, DownloadState};
use std::sync::Arc;
use tracing::{error, info, warn};

/// Reconcile stale download sessions on app startup
///
/// Finds downloads stuck in "Downloading" state (from app crash)
/// and marks them as Failed to prevent zombie states.
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

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
mod tests {
    use super::*;
    use crate::features::download::download_repository::mock::MockDownloadRepository;
    use lattice_core::domain::download::DownloadSession;

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

    #[tokio::test]
    async fn sqlite_session_interrupted_before_relaunch_is_reconciled() {
        use crate::features::download::download_repository::SqliteDownloadRepository;
        use crate::infrastructure::persistence::database::connection::DatabaseConnection;

        let dir = tempfile::tempdir().unwrap();
        let conn = Arc::new(
            DatabaseConnection::new(dir.path().join("reconcile.db"))
                .await
                .unwrap(),
        );
        sqlx::migrate!("./migrations")
            .run(conn.pool())
            .await
            .unwrap();
        let repository: Arc<dyn DownloadRepository> = Arc::new(SqliteDownloadRepository::new(conn));

        let mut session = downloading("sqlite-interrupted");
        session.update_progress(500, 100.0);
        repository.create(&session).await.unwrap();

        let startup = Utc::now() + chrono::Duration::milliseconds(10);
        let reconciled = reconcile_stale_downloads(repository.clone(), startup)
            .await
            .unwrap();

        assert_eq!(
            reconciled, 1,
            "SqliteDownloadRepository must preserve updated_at so stale Downloading rows are reconciled"
        );
        let updated = repository.get("sqlite-interrupted").await.unwrap().unwrap();
        assert_eq!(updated.state(), &DownloadState::Failed);
        assert_eq!(
            updated.error_message(),
            Some("App restarted while downloading")
        );
    }
}
