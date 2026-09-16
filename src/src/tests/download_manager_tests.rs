#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]

#[cfg(feature = "test-utils")]
use lattice::features::download::download_repository::mock::MockDownloadRepository;
#[cfg(feature = "test-utils")]
use lattice::features::download::engine::mock::MockDownloadEngine;
#[cfg(feature = "test-utils")]
use lattice::{
    domain::download::{Checksum, ChecksumAlgorithm, DownloadState},
    features::download::manager::{DownloadManager, DownloadManagerService, DownloadRequest},
};
#[cfg(feature = "test-utils")]
use std::{path::PathBuf, sync::Arc};
#[cfg(feature = "test-utils")]
use tokio::time::{sleep, Duration};

#[tokio::test]
#[cfg(feature = "test-utils")]
async fn test_download_manager_start_download() {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager =
        DownloadManagerService::new(repository.clone(), engine.clone(), PathBuf::from("/tmp"));

    engine.set_file_size("https://example.com/file.bin", Some(1000));

    let id = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file.bin".to_string(),
            destination: PathBuf::from("/tmp/file.bin"),
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
#[cfg(feature = "test-utils")]
async fn test_download_manager_pause_and_resume() {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager =
        DownloadManagerService::new(repository.clone(), engine.clone(), PathBuf::from("/tmp"));

    engine.set_file_size("https://example.com/file.bin", Some(1000));
    engine.set_stall(true);

    let id = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file.bin".to_string(),
            destination: PathBuf::from("/tmp/file.bin"),
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
#[cfg(feature = "test-utils")]
async fn test_download_manager_cancel() {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager =
        DownloadManagerService::new(repository.clone(), engine.clone(), PathBuf::from("/tmp"));

    engine.set_file_size("https://example.com/file.bin", Some(1000));
    engine.set_stall(true);

    let id = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file.bin".to_string(),
            destination: PathBuf::from("/tmp/file.bin"),
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
#[cfg(feature = "test-utils")]
async fn test_download_manager_list_downloads() {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager =
        DownloadManagerService::new(repository.clone(), engine.clone(), PathBuf::from("/tmp"));

    engine.set_file_size("https://example.com/file1.bin", Some(1000));
    engine.set_file_size("https://example.com/file2.bin", Some(2000));

    let id1 = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file1.bin".to_string(),
            destination: PathBuf::from("/tmp/file1.bin"),
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
            destination: PathBuf::from("/tmp/file2.bin"),
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
#[cfg(feature = "test-utils")]
async fn test_download_manager_with_checksum() {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager =
        DownloadManagerService::new(repository.clone(), engine.clone(), PathBuf::from("/tmp"));

    engine.set_file_size("https://example.com/file.bin", Some(1000));

    let valid_checksum = "a".repeat(64);
    let checksum = Checksum::new(ChecksumAlgorithm::Sha256, valid_checksum).unwrap();

    let id = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file.bin".to_string(),
            destination: PathBuf::from("/tmp/file.bin"),
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
#[cfg(feature = "test-utils")]
async fn test_download_manager_concurrent_downloads() {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager =
        DownloadManagerService::new(repository.clone(), engine.clone(), PathBuf::from("/tmp"))
            .with_max_concurrent(2);

    engine.set_file_size("https://example.com/file1.bin", Some(1000));
    engine.set_file_size("https://example.com/file2.bin", Some(2000));
    engine.set_file_size("https://example.com/file3.bin", Some(3000));

    let _id1 = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file1.bin".to_string(),
            destination: PathBuf::from("/tmp/file1.bin"),
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
            destination: PathBuf::from("/tmp/file2.bin"),
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
            destination: PathBuf::from("/tmp/file3.bin"),
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
#[cfg(feature = "test-utils")]
async fn test_download_manager_event_subscription() {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager =
        DownloadManagerService::new(repository.clone(), engine.clone(), PathBuf::from("/tmp"));

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
            destination: PathBuf::from("/tmp/file.bin"),
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
#[cfg(feature = "test-utils")]
async fn test_download_manager_error_handling() {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager =
        DownloadManagerService::new(repository.clone(), engine.clone(), PathBuf::from("/tmp"));

    let result = manager.pause_download("non-existent-id").await;

    assert!(result.is_err());
}

#[tokio::test]
#[cfg(feature = "test-utils")]
async fn test_download_manager_state_persistence() {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager =
        DownloadManagerService::new(repository.clone(), engine.clone(), PathBuf::from("/tmp"));

    engine.set_file_size("https://example.com/file.bin", Some(1000));
    engine.set_stall(true);

    let id = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file.bin".to_string(),
            destination: PathBuf::from("/tmp/file.bin"),
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

    let manager2 =
        DownloadManagerService::new(repository.clone(), engine.clone(), PathBuf::from("/tmp"));

    let status_after = manager2.get_download_status(&id).await.unwrap().unwrap();

    assert_eq!(status_before.id(), status_after.id());
    assert_eq!(status_before.url(), status_after.url());
}
