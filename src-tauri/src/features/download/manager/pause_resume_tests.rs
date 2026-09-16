//! Regression tests for pause/resume: a pause must stay a pause, must give
//! its concurrency slot back, and a late progress tick must not resurrect a
//! terminal session.

use crate::domain::download::DownloadState;
use crate::features::download::download_repository::mock::MockDownloadRepository;
use crate::features::download::download_repository::DownloadRepository;
use crate::features::download::engine::mock::MockDownloadEngine;
use crate::features::download::manager::{
    DownloadManager, DownloadManagerService, DownloadRequest,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::Duration;

fn temp_root() -> PathBuf {
    std::env::temp_dir().join("lattice-download-pause-tests")
}

async fn start_one(
    manager: &DownloadManagerService,
    engine: &MockDownloadEngine,
    name: &str,
) -> String {
    let url = format!("https://example.com/{}", name);
    engine.set_file_size(&url, Some(10_000_000));
    // Keep the transfer in flight so pause/cancel act on a live task
    // rather than racing an instant completion.
    engine.set_stall(true);
    manager
        .start_download(DownloadRequest {
            url,
            destination: temp_root().join(name),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        })
        .await
        .expect("start download")
}

/// Pausing must leave the session in `Paused`. It used to reach the task's
/// stop branch, which unconditionally ran `session.cancel()` — and since
/// `Paused → Cancelled` is a legal transition, the row ended up
/// `Cancelled`. `resume_download` only accepts `Paused | Failed`, so
/// resume then refused forever and the only working button, retry,
/// deletes the partial file.
#[tokio::test]
async fn pausing_leaves_the_session_paused_and_resumable() {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(repository.clone(), engine.clone(), temp_root());

    let id = start_one(&manager, &engine, "paused.bin").await;

    manager.pause_download(&id).await.expect("pause");

    // Give the task time to observe the stop signal and unwind.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let session = repository
        .get(&id)
        .await
        .expect("get")
        .expect("session exists");
    assert_eq!(
        session.state(),
        &DownloadState::Paused,
        "pause must not turn into cancel when the task tears down"
    );

    // And resume must be accepted rather than rejected.
    manager
        .resume_download(&id)
        .await
        .expect("resume must be accepted after a pause");
}

/// A paused download must give its concurrency slot back. The gate is
/// `active.len() >= max_concurrent_downloads` (2), so leaking slots meant
/// that pausing two files silently wedged every queued and future
/// download until the app restarted.
#[tokio::test]
async fn pausing_releases_the_concurrency_slot() {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(repository.clone(), engine.clone(), temp_root());

    let first = start_one(&manager, &engine, "slot-a.bin").await;
    let second = start_one(&manager, &engine, "slot-b.bin").await;

    manager.pause_download(&first).await.expect("pause first");
    manager.pause_download(&second).await.expect("pause second");

    tokio::time::sleep(Duration::from_millis(200)).await;

    let active = manager.active_downloads.read().await;
    assert!(
        active.is_empty(),
        "paused downloads must not keep occupying slots, still held: {:?}",
        active.keys().collect::<Vec<_>>()
    );
    drop(active);

    // A third download must therefore be able to start.
    let third = start_one(&manager, &engine, "slot-c.bin").await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let session = repository
        .get(&third)
        .await
        .expect("get")
        .expect("third session exists");
    assert_ne!(
        session.state(),
        &DownloadState::Pending,
        "a third download must start once slots are freed, got {:?}",
        session.state()
    );
}

/// A progress tick that lands after a terminal transition must not revive
/// the row. This is what left sessions stuck as forever-downloading, which
/// startup reconciliation then marked `failed` even though the model had
/// completed and been registered.
#[tokio::test]
async fn progress_write_cannot_revert_a_terminal_state() {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(repository.clone(), engine.clone(), temp_root());

    let id = start_one(&manager, &engine, "terminal.bin").await;

    // Drive the session to a terminal state...
    manager.cancel_download(&id).await.expect("cancel");
    tokio::time::sleep(Duration::from_millis(100)).await;

    // ...then deliver a late progress tick, as a detached task would.
    repository
        .update_progress(&id, 9_999, 1234.0)
        .await
        .expect("late progress tick");

    let session = repository
        .get(&id)
        .await
        .expect("get")
        .expect("session exists");
    assert_eq!(
        session.state(),
        &DownloadState::Cancelled,
        "a late progress tick must not resurrect a terminal session"
    );
}
