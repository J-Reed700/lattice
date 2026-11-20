use crate::features::download::download_repository::mock::MockDownloadRepository;
use crate::features::download::engine::mock::MockDownloadEngine;
use crate::features::download::manager::DownloadManagerService;
use lattice_core::application::ports::download_manager::{DownloadManager, DownloadRequest};
use lattice_core::domain::download::{Checksum, ChecksumAlgorithm, DownloadState};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_download_manager_start_download() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(
        repository.clone(),
        engine.clone(),
        temp.path().to_path_buf(),
        Arc::default(),
    );

    engine.set_file_size("https://example.com/file.bin", Some(1000));

    let id = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file.bin".to_string(),
            destination: temp.path().join("file.bin"),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        })
        .await
        .unwrap();

    assert!(!id.is_empty());

    sleep(Duration::from_millis(100)).await;

    let status = manager.get_download_status(&id).await.unwrap();
    assert!(status.is_some());

    let session = status.unwrap();
    assert_eq!(session.id(), id);
    assert_eq!(session.url(), "https://example.com/file.bin");
}

#[tokio::test]
async fn test_download_manager_pause_and_resume() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(
        repository.clone(),
        engine.clone(),
        temp.path().to_path_buf(),
        Arc::default(),
    );

    engine.set_file_size("https://example.com/file.bin", Some(1000));
    engine.set_stall(true);

    let id = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file.bin".to_string(),
            destination: temp.path().join("file.bin"),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        })
        .await
        .unwrap();

    sleep(Duration::from_millis(50)).await;

    manager.pause_download(&id).await.unwrap();

    let status = manager.get_download_status(&id).await.unwrap().unwrap();
    assert!(
        status.state() == &DownloadState::Paused || status.state() == &DownloadState::Completed
    );

    if status.state() == &DownloadState::Paused {
        manager.resume_download(&id).await.unwrap();

        sleep(Duration::from_millis(50)).await;

        let resumed_status = manager.get_download_status(&id).await.unwrap().unwrap();
        assert!(
            resumed_status.state() == &DownloadState::Downloading
                || resumed_status.state() == &DownloadState::Completed
        );
    }
}

#[tokio::test]
async fn test_download_manager_cancel() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(
        repository.clone(),
        engine.clone(),
        temp.path().to_path_buf(),
        Arc::default(),
    );

    engine.set_file_size("https://example.com/file.bin", Some(1000));
    engine.set_stall(true);

    let id = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file.bin".to_string(),
            destination: temp.path().join("file.bin"),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        })
        .await
        .unwrap();

    sleep(Duration::from_millis(50)).await;

    manager.cancel_download(&id).await.unwrap();

    let status = manager.get_download_status(&id).await.unwrap().unwrap();
    assert!(
        status.state() == &DownloadState::Cancelled || status.state() == &DownloadState::Completed
    );
}

#[tokio::test]
async fn test_download_manager_list_downloads() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(
        repository.clone(),
        engine.clone(),
        temp.path().to_path_buf(),
        Arc::default(),
    );

    engine.set_file_size("https://example.com/file1.bin", Some(1000));
    engine.set_file_size("https://example.com/file2.bin", Some(2000));

    let id1 = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file1.bin".to_string(),
            destination: temp.path().join("file1.bin"),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        })
        .await
        .unwrap();

    let id2 = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file2.bin".to_string(),
            destination: temp.path().join("file2.bin"),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        })
        .await
        .unwrap();

    sleep(Duration::from_millis(100)).await;

    let downloads = manager.list_downloads().await.unwrap();
    assert!(downloads.len() >= 2);

    let ids: Vec<String> = downloads.iter().map(|d| d.id().to_string()).collect();
    assert!(ids.contains(&id1));
    assert!(ids.contains(&id2));
}

#[tokio::test]
async fn test_download_manager_with_checksum() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(
        repository.clone(),
        engine.clone(),
        temp.path().to_path_buf(),
        Arc::default(),
    );

    engine.set_file_size("https://example.com/file.bin", Some(1000));

    let valid_checksum = "a".repeat(64);
    let checksum = Checksum::new(ChecksumAlgorithm::Sha256, valid_checksum).unwrap();

    let id = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file.bin".to_string(),
            destination: temp.path().join("file.bin"),
            checksum: Some(checksum),
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        })
        .await
        .unwrap();

    assert!(!id.is_empty());

    sleep(Duration::from_millis(100)).await;

    let status = manager.get_download_status(&id).await.unwrap();
    assert!(status.is_some());
}

#[tokio::test]
async fn test_download_manager_concurrent_downloads() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(
        repository.clone(),
        engine.clone(),
        temp.path().to_path_buf(),
        Arc::default(),
    )
    .with_max_concurrent(2);

    engine.set_file_size("https://example.com/file1.bin", Some(1000));
    engine.set_file_size("https://example.com/file2.bin", Some(2000));
    engine.set_file_size("https://example.com/file3.bin", Some(3000));

    let _id1 = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file1.bin".to_string(),
            destination: temp.path().join("file1.bin"),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        })
        .await
        .unwrap();

    let _id2 = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file2.bin".to_string(),
            destination: temp.path().join("file2.bin"),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        })
        .await
        .unwrap();

    let _id3 = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file3.bin".to_string(),
            destination: temp.path().join("file3.bin"),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        })
        .await
        .unwrap();

    sleep(Duration::from_millis(150)).await;

    let active = manager.list_active_downloads().await.unwrap();
    assert!(active.len() <= 2, "Should respect max concurrent limit");

    let all = manager.list_downloads().await.unwrap();
    assert!(all.len() >= 3);
}

#[tokio::test]
async fn test_download_manager_event_subscription() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(
        repository.clone(),
        engine.clone(),
        temp.path().to_path_buf(),
        Arc::default(),
    );

    engine.set_file_size("https://example.com/file.bin", Some(1000));

    let event_rx_arc = manager.subscribe_to_events();

    let manager_clone = Arc::new(manager);
    let manager_for_task = manager_clone.clone();

    let event_task = tokio::spawn(async move {
        let mut events = Vec::new();
        let mut timeout_count = 0;
        const MAX_TIMEOUTS: usize = 10;

        // Take the receiver from the Arc<RwLock<Option<...>>>
        let mut event_rx = {
            let mut rx_opt = event_rx_arc.write().await;
            match rx_opt.take() {
                Some(rx) => rx,
                None => return events, // Receiver already taken, return empty
            }
        };

        while timeout_count < MAX_TIMEOUTS {
            match tokio::time::timeout(Duration::from_millis(100), event_rx.recv()).await {
                Ok(Some(_event)) => {
                    events.push(());
                    timeout_count = 0;
                }
                Ok(None) => break,
                Err(_) => {
                    timeout_count += 1;
                }
            }
        }

        events
    });

    sleep(Duration::from_millis(50)).await;

    let _id = manager_for_task
        .start_download(DownloadRequest {
            url: "https://example.com/file.bin".to_string(),
            destination: temp.path().join("file.bin"),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        })
        .await
        .unwrap();

    sleep(Duration::from_millis(200)).await;

    let events = tokio::time::timeout(Duration::from_secs(2), event_task)
        .await
        .expect("Event task timed out")
        .expect("Event task panicked");

    assert!(!events.is_empty(), "Should receive at least one event");
}

#[tokio::test]
async fn test_download_manager_error_handling() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(
        repository.clone(),
        engine.clone(),
        temp.path().to_path_buf(),
        Arc::default(),
    );

    let result = manager.pause_download("non-existent-id").await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_download_manager_state_persistence() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(
        repository.clone(),
        engine.clone(),
        temp.path().to_path_buf(),
        Arc::default(),
    );

    engine.set_file_size("https://example.com/file.bin", Some(1000));
    engine.set_stall(true);

    let id = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file.bin".to_string(),
            destination: temp.path().join("file.bin"),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        })
        .await
        .unwrap();

    sleep(Duration::from_millis(50)).await;

    manager.pause_download(&id).await.unwrap();

    let status_before = manager.get_download_status(&id).await.unwrap().unwrap();

    let manager2 = DownloadManagerService::new(
        repository.clone(),
        engine.clone(),
        temp.path().to_path_buf(),
        Arc::default(),
    );

    let status_after = manager2.get_download_status(&id).await.unwrap().unwrap();

    assert_eq!(status_before.id(), status_after.id());
    assert_eq!(status_before.url(), status_after.url());
}

#[tokio::test]
async fn test_delete_pending_by_model_clears_queue_and_auth_tokens() {
    use crate::features::download::download_repository::DownloadRepository;
    use lattice_core::domain::download::DownloadSession;

    let temp = tempfile::tempdir().unwrap();
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(
        repository.clone(),
        engine,
        temp.path().to_path_buf(),
        Arc::default(),
    );

    let target = DownloadSession::new(
        "pending-target".to_string(),
        "https://example.com/a.gguf".to_string(),
        temp.path().join("a.gguf"),
        Some(100),
        None,
    )
    .unwrap()
    .with_model_metadata("Model A".to_string(), "model-a".to_string());
    let other = DownloadSession::new(
        "pending-other".to_string(),
        "https://example.com/b.gguf".to_string(),
        temp.path().join("b.gguf"),
        Some(100),
        None,
    )
    .unwrap()
    .with_model_metadata("Model B".to_string(), "model-b".to_string());
    repository.create(&target).await.unwrap();
    repository.create(&other).await.unwrap();

    {
        let mut tokens = manager.auth_tokens.write().await;
        tokens.insert("pending-target".to_string(), "secret-a".to_string());
        tokens.insert("pending-other".to_string(), "secret-b".to_string());
    }
    {
        let mut queue = manager.download_queue.write().await;
        queue.push_back("pending-target".to_string());
        queue.push_back("pending-other".to_string());
    }

    let deleted = manager.delete_pending_by_model("model-a").await.unwrap();
    assert_eq!(deleted, 1);

    let tokens = manager.auth_tokens.read().await;
    assert!(!tokens.contains_key("pending-target"));
    assert_eq!(
        tokens.get("pending-other").map(String::as_str),
        Some("secret-b")
    );

    let queue = manager.download_queue.read().await;
    assert!(!queue.iter().any(|id| id == "pending-target"));
    assert!(queue.iter().any(|id| id == "pending-other"));
}

#[tokio::test]
async fn test_failed_queue_start_clears_auth_token() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(
        repository,
        engine,
        temp.path().to_path_buf(),
        Arc::default(),
    );

    // Queued session missing from repository makes `start_download_task` return `Err(SessionNotFound)`.
    manager
        .auth_tokens
        .write()
        .await
        .insert("missing-session".to_string(), "secret-token".to_string());
    manager
        .download_queue
        .write()
        .await
        .push_back("missing-session".to_string());

    manager.process_pending_queue().await.unwrap();

    assert!(
        !manager
            .auth_tokens
            .read()
            .await
            .contains_key("missing-session"),
        "failed start_download_task must evict auth token"
    );
}
