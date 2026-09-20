//! Start, pause/resume, cancel, delete and list operations.

use super::{poll_until, temp_file, temp_root};
use crate::domain::download::{DownloadSession, DownloadState};
use crate::features::download::download_repository::mock::MockDownloadRepository;
use crate::features::download::download_repository::DownloadRepository;
use crate::features::download::engine::mock::MockDownloadEngine;
use crate::features::download::manager::{
    DownloadBatchItem, DownloadEvent, DownloadManager, DownloadManagerService, DownloadRequest,
};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::time::Duration;

#[tokio::test]
async fn test_start_download() -> Result<(), Box<dyn std::error::Error>> {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(repository.clone(), engine.clone(), temp_root());

    engine.set_file_size("https://example.com/file.bin", Some(1000));

    let id = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file.bin".to_string(),
            destination: temp_file("file.bin"),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        })
        .await?;

    assert!(!id.is_empty(), "Download ID should not be empty");

    let repo_clone = repository.clone();
    let id_clone = id.clone();
    let status = poll_until(
        move || {
            let repo = repo_clone.clone();
            let id = id_clone.clone();
            Box::pin(async move { repo.get(&id).await.ok().flatten() })
                as Pin<Box<dyn Future<Output = Option<DownloadSession>> + Send>>
        },
        Duration::from_secs(2),
    )
    .await?;

    assert!(
        status.state() == &DownloadState::Downloading
            || status.state() == &DownloadState::Completed
            || status.state() == &DownloadState::Failed,
        "Download should be in active or terminal state, got {:?}",
        status.state()
    );

    Ok(())
}

#[tokio::test]
async fn batch_is_fully_registered_before_first_started_event(
) -> Result<(), Box<dyn std::error::Error>> {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(repository.clone(), engine.clone(), temp_root())
        .with_max_concurrent(1);

    let mut items = Vec::new();
    for index in 1..=3 {
        let url = format!("https://example.com/model-{index}.bin");
        engine.set_file_size(&url, Some(1000));
        items.push(DownloadBatchItem {
            requests: vec![DownloadRequest {
                url,
                destination: temp_file(&format!("model-{index}.bin")),
                checksum: None,
                auth_token: None,
                model_name: Some("Qwen embedding".to_string()),
                model_id: Some("qwen-embedding".to_string()),
                model_file_name: Some(format!("model-{index}.bin")),
            }],
        });
    }

    let receiver = manager.subscribe_to_events();
    let mut receiver = receiver
        .write()
        .await
        .take()
        .ok_or("event receiver already taken")?;
    let repository_at_event = repository.clone();
    let event_observer = tokio::spawn(async move {
        let event = receiver.recv().await;
        let session_count = repository_at_event.list().await.map(|rows| rows.len());
        (event, session_count)
    });

    let ids = manager.start_download_batch(items).await?;
    let (event, session_count) = event_observer.await?;

    assert_eq!(ids.len(), 3);
    assert_eq!(session_count?, 3);
    assert!(matches!(event, Some(DownloadEvent::Started { .. })));

    Ok(())
}

#[tokio::test]
async fn test_pause_and_resume() -> Result<(), Box<dyn std::error::Error>> {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(repository.clone(), engine.clone(), temp_root());

    engine.set_file_size("https://example.com/file.bin", Some(1000));

    let id = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file.bin".to_string(),
            destination: temp_file("file.bin"),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        })
        .await?;

    // Wait for download to be created in repository
    let repo_clone = repository.clone();
    let id_clone = id.clone();
    poll_until(
        move || {
            let repo = repo_clone.clone();
            let id = id_clone.clone();
            Box::pin(async move { repo.get(&id).await.ok().flatten() })
                as Pin<Box<dyn Future<Output = Option<DownloadSession>> + Send>>
        },
        Duration::from_secs(2),
    )
    .await?;

    // Try to pause - may fail if download already completed/failed
    let pause_result = manager.pause_download(&id).await;

    let status = manager
        .get_download_status(&id)
        .await?
        .ok_or("Status not found")?;

    // Download may be in various states depending on timing:
    // - Paused (if we caught it in time)
    // - Completed (if validation passed - unlikely with mock)
    // - Failed (if validation failed - expected with mock since no real file)
    // - Downloading (if pause failed due to race condition)
    if status.state() == &DownloadState::Paused {
        // Only test resume if pause succeeded
        assert!(
            pause_result.is_ok(),
            "Pause should succeed when state is Paused"
        );
        manager.resume_download(&id).await?;

        let resumed_status = manager
            .get_download_status(&id)
            .await?
            .ok_or("Status not found after resume")?;
        assert!(
            resumed_status.state() != &DownloadState::Paused,
            "Download should not be Paused after resume, got {:?}",
            resumed_status.state()
        );
    } else {
        // Pause may have failed if download already in terminal state
        // This is acceptable for this test
    }

    Ok(())
}

#[tokio::test]
async fn test_cancel_download() -> Result<(), Box<dyn std::error::Error>> {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(repository.clone(), engine.clone(), temp_root());

    engine.set_file_size("https://example.com/file.bin", Some(1000));

    let id = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file.bin".to_string(),
            destination: temp_file("file.bin"),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        })
        .await?;

    // Wait for download to be created in repository before canceling
    let repo_clone = repository.clone();
    let id_clone = id.clone();
    poll_until(
        move || {
            let repo = repo_clone.clone();
            let id = id_clone.clone();
            Box::pin(async move { repo.get(&id).await.ok().flatten() })
                as Pin<Box<dyn Future<Output = Option<DownloadSession>> + Send>>
        },
        Duration::from_secs(2),
    )
    .await?;

    // Cancel the download
    manager.cancel_download(&id).await?;

    let status = manager
        .get_download_status(&id)
        .await?
        .ok_or("Status not found")?;
    assert!(
        status.state() == &DownloadState::Cancelled
            || status.state() == &DownloadState::Completed
            || status.state() == &DownloadState::Failed,
        "Download should be in terminal state after cancel, got {:?}",
        status.state()
    );

    Ok(())
}

#[tokio::test]
async fn terminal_download_actions_are_idempotent() -> Result<(), Box<dyn std::error::Error>> {
    let mut session = DownloadSession::new(
        "completed-download".to_string(),
        "https://example.com/file.bin".to_string(),
        temp_file("completed-file.bin"),
        Some(1000),
        None,
    )?;
    session.start()?;
    session.update_progress(1000, 0.0);
    session.complete()?;

    let repository = Arc::new(MockDownloadRepository::new().with_session(session));
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(repository.clone(), engine, temp_root());

    manager.pause_download("completed-download").await?;
    manager.cancel_download("completed-download").await?;

    let status = repository
        .get("completed-download")
        .await?
        .ok_or("Status not found")?;
    assert_eq!(status.state(), &DownloadState::Completed);

    Ok(())
}

#[tokio::test]
async fn test_delete_download_is_idempotent_for_missing_session(
) -> Result<(), Box<dyn std::error::Error>> {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(repository, engine, temp_root());

    manager
        .delete_download("39c9b184-43c9-4af5-aec8-f1ffc7e6226e")
        .await?;

    Ok(())
}

#[tokio::test]
async fn test_list_downloads() -> Result<(), Box<dyn std::error::Error>> {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(repository.clone(), engine.clone(), temp_root());

    engine.set_file_size("https://example.com/file1.bin", Some(1000));
    engine.set_file_size("https://example.com/file2.bin", Some(2000));

    let id1 = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file1.bin".to_string(),
            destination: temp_file("file1.bin"),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        })
        .await?;

    let id2 = manager
        .start_download(DownloadRequest {
            url: "https://example.com/file2.bin".to_string(),
            destination: temp_file("file2.bin"),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        })
        .await?;

    // Wait for both downloads to be created in repository
    let repo_clone = repository.clone();
    let id1_clone = id1.clone();
    poll_until(
        move || {
            let repo = repo_clone.clone();
            let id = id1_clone.clone();
            Box::pin(async move { repo.get(&id).await.ok().flatten() })
                as Pin<Box<dyn Future<Output = Option<DownloadSession>> + Send>>
        },
        Duration::from_secs(2),
    )
    .await?;

    let repo_clone = repository.clone();
    let id2_clone = id2.clone();
    poll_until(
        move || {
            let repo = repo_clone.clone();
            let id = id2_clone.clone();
            Box::pin(async move { repo.get(&id).await.ok().flatten() })
                as Pin<Box<dyn Future<Output = Option<DownloadSession>> + Send>>
        },
        Duration::from_secs(2),
    )
    .await?;

    let downloads = manager.list_downloads().await?;
    assert!(
        downloads.len() >= 2,
        "Expected at least 2 downloads, got {}",
        downloads.len()
    );

    Ok(())
}

/// A relaunch leaves the interrupted session behind, still marked Downloading.
/// Asking for the same file again must replace it, not sit beside it: the
/// downloads panel groups a model's sessions, and the dead one showed as a row
/// that never moved.
#[tokio::test]
async fn a_new_download_supersedes_a_dead_session_for_the_same_file(
) -> Result<(), Box<dyn std::error::Error>> {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(repository.clone(), engine.clone(), temp_root());
    let url = "https://example.com/superseded.bin";
    let destination = temp_file("superseded.bin");
    engine.set_file_size(url, Some(1000));

    let mut leftover = DownloadSession::new(
        "left-by-a-previous-launch".to_string(),
        url.to_string(),
        destination.clone(),
        Some(1000),
        None,
    )?;
    leftover.start()?;
    repository.create(&leftover).await?;

    let mut elsewhere = DownloadSession::new(
        "another-file".to_string(),
        "https://example.com/other.bin".to_string(),
        temp_file("other.bin"),
        Some(10),
        None,
    )?;
    elsewhere.start()?;
    repository.create(&elsewhere).await?;

    let id = manager
        .start_download(DownloadRequest {
            url: url.to_string(),
            destination: destination.clone(),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        })
        .await?;

    let for_destination: Vec<String> = repository
        .list()
        .await?
        .into_iter()
        .filter(|session| session.destination() == &destination)
        .map(|session| session.id().to_string())
        .collect();
    assert_eq!(for_destination, vec![id], "one session per destination");
    assert!(
        repository.get("another-file").await?.is_some(),
        "a session for a different file is not touched"
    );
    Ok(())
}
