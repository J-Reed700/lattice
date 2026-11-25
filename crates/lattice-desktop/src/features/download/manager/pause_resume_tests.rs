//! Regression tests for pause/resume: a pause must stay a pause, must give
//! its concurrency slot back, and a late progress tick must not resurrect a
//! terminal session.

use crate::features::download::download_repository::mock::MockDownloadRepository;
use crate::features::download::download_repository::DownloadRepository;
use crate::features::download::engine::mock::MockDownloadEngine;
use crate::features::download::manager::DownloadManagerService;
use lattice_core::application::ports::download_manager::{DownloadManager, DownloadRequest};
use lattice_core::domain::download::DownloadState;
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
            expected_bytes: None,
        })
        .await
        .expect("start download")
}

/// Pausing leaves the session `Paused`, not `Cancelled`, so
/// `resume_download` (which accepts `Paused | Failed`) can resume it.
#[tokio::test]
async fn pausing_leaves_the_session_paused_and_resumable() {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(
        repository.clone(),
        engine.clone(),
        temp_root(),
        Arc::default(),
    );

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

    let calls_before_resume = engine.get_download_calls().len();

    // Resume must not only accept the state transition, it must actually spawn
    // a new worker task and invoke the download engine again.
    manager
        .resume_download(&id)
        .await
        .expect("resume must be accepted after a pause");

    assert!(
        manager.active_downloads.read().await.contains_key(&id),
        "resumed download must register an active transfer task"
    );

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if engine.get_download_calls().len() > calls_before_resume {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("resumed download must invoke the download engine");
}

/// A paused download must give its concurrency slot back. The gate is
/// `active.len() >= max_concurrent_downloads` (2), so leaking slots meant
/// that pausing two files silently wedged every queued and future
/// download until the app restarted.
#[tokio::test]
async fn pausing_releases_the_concurrency_slot() {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(
        repository.clone(),
        engine.clone(),
        temp_root(),
        Arc::default(),
    );

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
    let manager = DownloadManagerService::new(
        repository.clone(),
        engine.clone(),
        temp_root(),
        Arc::default(),
    );

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

#[tokio::test]
async fn slow_progress_persistence_coalesces_flood_and_flushes_before_pause() {
    let repository =
        Arc::new(MockDownloadRepository::new().with_progress_delay(Duration::from_millis(30)));
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(
        repository.clone(),
        engine.clone(),
        temp_root(),
        Arc::default(),
    );
    let id = start_one(&manager, &engine, "coalesced.bin").await;
    let callback = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(call) = engine.get_last_call() {
                if let Some(callback) = call.progress_callback {
                    break callback;
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("transfer starts");
    // No await: the callback must have constant storage and not spawn work.
    for bytes in 1..=100_000 {
        callback(bytes, 1024.0);
    }
    manager.pause_download(&id).await.unwrap();
    let paused = repository.get(&id).await.unwrap().unwrap();
    assert_eq!(paused.state(), &DownloadState::Paused);
    assert_eq!(paused.progress().bytes_downloaded(), 100_000);
    assert!(
        repository.progress_write_count() <= 3,
        "one latest progress slot, no callback backlog"
    );
    let writes = repository.progress_write_count();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        repository.progress_write_count(),
        writes,
        "joined worker cannot write after pause"
    );
    assert!(manager.active_downloads.read().await.is_empty());
}

#[tokio::test]
async fn gated_huggingface_download_reloads_token_for_size_probe_pause_resume_retry_and_restart() {
    use lattice_core::application::ports::CredentialsPort;
    use lattice_core::application::services::credentials::{
        memory::InMemoryCredentials, CredentialKey,
    };

    let dir = tempfile::tempdir().unwrap();
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let credentials = Arc::new(InMemoryCredentials::default());
    credentials
        .set_api_key(CredentialKey::HuggingFaceToken.as_str(), "hf_secret_123")
        .await
        .unwrap();

    let hf_url = "https://huggingface.co/meta-llama/Llama-3.3-70B/resolve/main/model.gguf";
    engine.set_file_size(hf_url, Some(2000));
    engine.set_stall(true);

    let manager = DownloadManagerService::new(
        repository.clone(),
        engine.clone(),
        dir.path().to_path_buf(),
        Arc::default(),
    )
    .with_credentials(credentials.clone());

    let id = manager
        .start_download(DownloadRequest {
            url: hf_url.to_string(),
            destination: dir.path().join("model.gguf"),
            checksum: None,
            auth_token: None,
            model_name: Some("Llama 3.3".to_string()),
            model_id: Some("meta-llama/Llama-3.3-70B".to_string()),
            model_file_name: Some("model.gguf".to_string()),
            expected_bytes: None,
        })
        .await
        .unwrap();

    // Size probe (prepare_download -> get_file_size) received the HF token.
    assert_eq!(
        engine.get_file_size_calls(),
        vec![(hf_url.to_string(), Some("hf_secret_123".to_string()))]
    );

    // Wait for initial download() call and verify token.
    tokio::time::timeout(Duration::from_secs(1), async {
        while engine.get_download_calls().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        engine.get_last_call().unwrap().auth_token,
        Some("hf_secret_123".to_string())
    );

    // Pause clears in-memory auth_tokens, then resume reloads from CredentialsPort.
    manager.pause_download(&id).await.unwrap();
    assert!(!manager.auth_tokens.read().await.contains_key(&id));
    engine.clear_calls();
    manager.resume_download(&id).await.unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while engine.get_download_calls().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        engine.get_last_call().unwrap().auth_token,
        Some("hf_secret_123".to_string())
    );

    // Simulate app restart: new DownloadManagerService (empty auth_tokens) resumes Paused session.
    manager.pause_download(&id).await.unwrap();
    drop(manager);

    let restarted_engine = Arc::new(MockDownloadEngine::new());
    restarted_engine.set_file_size(hf_url, Some(2000));
    restarted_engine.set_write_len(Some(2000));
    let restarted_manager = DownloadManagerService::new(
        repository.clone(),
        restarted_engine.clone(),
        dir.path().to_path_buf(),
        Arc::default(),
    )
    .with_credentials(credentials.clone());

    restarted_manager.resume_download(&id).await.unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while restarted_engine.get_download_calls().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        restarted_engine.get_last_call().unwrap().auth_token,
        Some("hf_secret_123".to_string())
    );

    // Also test retry_download after failure reloads the token.
    let mut failed_session = lattice_core::domain::download::DownloadSession::new(
        "failed-hf".to_string(),
        hf_url.to_string(),
        dir.path().join("failed.gguf"),
        Some(2000),
        None,
    )
    .unwrap();
    failed_session.start().unwrap();
    failed_session.fail("401".to_string()).unwrap();
    repository.create(&failed_session).await.unwrap();

    restarted_engine.clear_calls();
    restarted_engine.set_stall(true);
    restarted_manager.retry_download("failed-hf").await.unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while restarted_engine.get_download_calls().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        restarted_engine.get_last_call().unwrap().auth_token,
        Some("hf_secret_123".to_string())
    );
}
