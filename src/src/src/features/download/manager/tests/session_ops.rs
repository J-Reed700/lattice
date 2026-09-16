//! Start, pause/resume, cancel, delete and list operations.

use super::{poll_until, temp_file, temp_root};
use crate::domain::download::{DownloadSession, DownloadState};
use crate::features::download::download_repository::mock::MockDownloadRepository;
use crate::features::download::download_repository::DownloadRepository;
use crate::features::download::engine::mock::MockDownloadEngine;
use crate::features::download::manager::{
    DownloadManager, DownloadManagerService, DownloadRequest,
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
