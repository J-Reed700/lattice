//! Download session rows left behind by the last run: dead downloads and orphaned sessions.

use crate::features::download::download_repository::DownloadRepository;
use crate::infrastructure::persistence::repositories::downloaded_models::DownloadedModelRepository;
use chrono::{DateTime, Utc};
use lattice_core::domain::download::{DownloadError, DownloadState};
use std::sync::Arc;
use tracing::{error, info, warn};

/// Reconcile stale download sessions on app startup
///
/// Finds downloads stuck in "Downloading" state (from app crash) or still
/// "Pending" (queued in memory only, so a restart loses them). Both are marked
/// Paused (preserving `.part` byte progress if present on disk, or 0 if
/// missing) so the user can resume them.
pub async fn reconcile_stale_downloads(
    repository: Arc<dyn DownloadRepository>,
    startup_ts: DateTime<Utc>,
) -> Result<usize, DownloadError> {
    info!("Starting download session reconciliation");

    let mut downloading_sessions = repository
        .list_by_state(&DownloadState::Downloading)
        .await?;
    downloading_sessions.extend(repository.list_by_state(&DownloadState::Pending).await?);

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

    warn!(
        "Found {} stale download(s) in 'Downloading' or 'Pending' state",
        count
    );

    let mut reconciled = 0;
    for mut session in stale_sessions {
        let session_id = session.id().to_string();

        // No transfer survives a restart; mark both Pending and Downloading
        // sessions Paused and align recorded progress with any `.part` on disk.
        let part_bytes = match tokio::fs::metadata(session.working_path()).await {
            Ok(meta) if meta.is_file() => {
                let len = meta.len();
                if session
                    .progress()
                    .total_bytes()
                    .is_some_and(|total| len > total)
                {
                    0
                } else {
                    len
                }
            }
            _ => 0,
        };
        session.update_progress(part_bytes, 0.0);

        match session.pause() {
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
                error!("Failed to pause stale session {}: {}", session_id, e);
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
/// The file stays: a completed download is a whole file, and a missing model
/// row is no evidence it is garbage. A vision projector has none by design,
/// and a restored backup or a migration can lose the row of a model still on
/// disk.
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

            match download_repo.delete(session.id()).await {
                Ok(_) => {
                    info!(
                        "Deleted orphaned session: {}; kept its file {:?}",
                        session_id,
                        session.destination()
                    );
                    cleaned += 1;
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
mod tests {
    use super::*;
    use crate::features::download::download_repository::mock::MockDownloadRepository;
    use lattice_core::domain::download::DownloadSession;
    use lattice_core::shared::persistence::Database;

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
        assert_eq!(session.state(), &DownloadState::Paused);
        assert_eq!(session.error_message(), None);
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

    /// A vision projector downloads with no model of its own, and a model row
    /// can be lost while its session stays. Neither makes a finished download
    /// garbage.
    #[tokio::test]
    async fn completed_downloads_keep_their_files_when_no_model_row_names_them() {
        let pool = lattice_core::shared::testing::sqlite::migrated_in_memory()
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let repository = Arc::new(MockDownloadRepository::new());
        let mut files = Vec::new();
        for (id, model) in [("projector", None), ("row-lost", Some("lost-model"))] {
            let destination = dir.path().join(format!("{id}.gguf"));
            std::fs::write(&destination, b"a finished download").unwrap();
            let mut session = DownloadSession::new(
                id.to_string(),
                format!("https://example.com/{id}.gguf"),
                destination.clone(),
                Some(19),
                None,
            )
            .unwrap();
            if let Some(model) = model {
                session = session.with_model_metadata(model.to_string(), model.to_string());
            }
            session.start().unwrap();
            session.complete().unwrap();
            repository.create(&session).await.unwrap();
            files.push(destination);
        }

        reconcile_orphaned_sessions(
            repository.clone(),
            Arc::new(DownloadedModelRepository::new(Database::new(pool))),
        )
        .await
        .unwrap();

        for file in &files {
            assert_eq!(
                std::fs::read(file).ok().as_deref(),
                Some(&b"a finished download"[..]),
                "{} was deleted",
                file.display()
            );
        }
    }

    #[tokio::test]
    async fn sqlite_session_interrupted_before_relaunch_is_reconciled() {
        use crate::features::download::download_repository::SqliteDownloadRepository;
        use crate::infrastructure::persistence::database::connection::DatabaseConnection;

        let dir = tempfile::tempdir().unwrap();
        let conn = DatabaseConnection::new(dir.path().join("reconcile.db"))
            .await
            .unwrap();
        crate::infrastructure::persistence::database::run_migrations(conn.database())
            .await
            .unwrap();
        let repository: Arc<dyn DownloadRepository> =
            Arc::new(SqliteDownloadRepository::new(conn.database().clone()));

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
        assert_eq!(updated.state(), &DownloadState::Paused);
        assert_eq!(updated.progress().bytes_downloaded(), 0);
        assert_eq!(updated.error_message(), None);
    }

    #[tokio::test]
    async fn interrupted_downloading_session_preserves_part_bytes_as_paused() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("model.gguf");
        std::fs::write(
            lattice_core::domain::download::part_path(&destination),
            vec![3u8; 640],
        )
        .unwrap();

        let mut session = DownloadSession::new(
            "with-part".to_string(),
            "https://example.com/model.gguf".to_string(),
            destination,
            Some(1000),
            None,
        )
        .unwrap();
        session.start().unwrap();
        session.update_progress(400, 100.0);

        let repository = Arc::new(MockDownloadRepository::new());
        repository.create(&session).await.unwrap();
        let startup = Utc::now() + chrono::Duration::milliseconds(5);

        let reconciled = reconcile_stale_downloads(repository.clone(), startup)
            .await
            .unwrap();

        assert_eq!(reconciled, 1);
        let after = repository.get("with-part").await.unwrap().unwrap();
        assert_eq!(after.state(), &DownloadState::Paused);
        assert_eq!(after.progress().bytes_downloaded(), 640);
        assert_eq!(after.error_message(), None);
    }

    #[tokio::test]
    async fn a_session_still_queued_at_a_relaunch_is_paused_not_failed() {
        let repository = Arc::new(MockDownloadRepository::new());
        let queued = DownloadSession::new(
            "queued".to_string(),
            "https://example.com/queued".to_string(),
            std::env::temp_dir().join("queued"),
            Some(1000),
            None,
        )
        .unwrap();
        repository.create(&queued).await.unwrap();
        repository
            .create(&downloading("interrupted"))
            .await
            .unwrap();
        let startup = Utc::now() + chrono::Duration::milliseconds(5);

        let reconciled = reconcile_stale_downloads(repository.clone(), startup)
            .await
            .unwrap();

        assert_eq!(reconciled, 2);
        let queued = repository.get("queued").await.unwrap().unwrap();
        assert_eq!(queued.state(), &DownloadState::Paused);
        assert_eq!(queued.error_message(), None);
        let interrupted = repository.get("interrupted").await.unwrap().unwrap();
        assert_eq!(interrupted.state(), &DownloadState::Paused);
        assert_eq!(interrupted.error_message(), None);
    }

    #[tokio::test]
    async fn a_pending_session_created_by_this_launch_is_left_queued() {
        let repository = Arc::new(MockDownloadRepository::new());
        let startup = Utc::now() - chrono::Duration::milliseconds(5);
        let queued = DownloadSession::new(
            "fresh".to_string(),
            "https://example.com/fresh".to_string(),
            std::env::temp_dir().join("fresh"),
            Some(1000),
            None,
        )
        .unwrap();
        repository.create(&queued).await.unwrap();

        let reconciled = reconcile_stale_downloads(repository.clone(), startup)
            .await
            .unwrap();

        assert_eq!(reconciled, 0);
        let session = repository.get("fresh").await.unwrap().unwrap();
        assert_eq!(session.state(), &DownloadState::Pending);
    }

    /// Resume queues a session as Pending, so a quit that leaves it waiting for
    /// a slot is a queued row at the next launch, not a dead transfer.
    #[tokio::test]
    async fn a_resumed_session_still_queued_at_shutdown_is_left_paused() {
        let repository = Arc::new(MockDownloadRepository::new());
        let mut session = downloading("queued-resume");
        session.pause().unwrap();
        session.resume().unwrap();
        assert_eq!(session.state(), &DownloadState::Pending);
        repository.create(&session).await.unwrap();
        let startup = Utc::now() + chrono::Duration::milliseconds(5);

        reconcile_stale_downloads(repository.clone(), startup)
            .await
            .unwrap();

        let after = repository.get("queued-resume").await.unwrap().unwrap();
        assert_eq!(after.state(), &DownloadState::Paused);
    }
}
