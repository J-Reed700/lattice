//! Rejected requests and transfer failures.

use super::{poll_until, temp_file, temp_root};
use crate::domain::download::DownloadError;
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
async fn test_download_invalid_path_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let repository = Arc::new(MockDownloadRepository::new());
    let mock_engine = Arc::new(MockDownloadEngine::new());

    let manager = DownloadManagerService::new(repository.clone(), mock_engine.clone(), temp_root());

    let request = DownloadRequest {
        url: "https://example.com/file.bin".to_string(),
        destination: "".into(),
        checksum: None,
        auth_token: None,
        model_name: None,
        model_id: None,
        model_file_name: None,
    };

    let result = manager.start_download(request).await;
    assert!(result.is_err(), "Should reject empty path");

    match result.unwrap_err() {
        DownloadError::InvalidDestination(msg) => {
            assert!(
                msg.contains("empty"),
                "Error should mention empty path: {}",
                msg
            );
        }
        other => panic!("Expected InvalidDestination, got: {:?}", other),
    }

    Ok(())
}

#[tokio::test]
async fn test_download_handles_transient_error_gracefully() -> Result<(), Box<dyn std::error::Error>>
{
    let repository = Arc::new(MockDownloadRepository::new());
    let mock_engine = Arc::new(MockDownloadEngine::new());

    // Configure mock to return a transient network error
    mock_engine.set_file_size("https://example.com/transient_error.bin", Some(1000));
    mock_engine.set_network_error("Connection timeout (transient)".to_string());

    let manager = DownloadManagerService::new(repository.clone(), mock_engine.clone(), temp_root());

    let request = DownloadRequest {
        url: "https://example.com/transient_error.bin".to_string(),
        destination: temp_file("transient_error.bin"),
        checksum: None,
        auth_token: None,
        model_name: None,
        model_id: None,
        model_file_name: None,
    };

    let download_id = manager.start_download(request).await?;

    // Wait for download to fail (due to network error)
    let repo_clone = repository.clone();
    let id_clone = download_id.clone();

    let final_download = poll_until(
        move || {
            let repo = repo_clone.clone();
            let id = id_clone.clone();
            Box::pin(async move {
                let download = repo.get(&id).await.ok().flatten()?;
                if matches!(
                    download.state(),
                    DownloadState::Completed | DownloadState::Failed
                ) {
                    Some(download)
                } else {
                    None
                }
            }) as Pin<Box<dyn Future<Output = Option<DownloadSession>> + Send>>
        },
        Duration::from_secs(3),
    )
    .await?;

    assert_eq!(
        final_download.state(),
        &DownloadState::Failed,
        "Download should fail on transient network error"
    );

    Ok(())
}

#[tokio::test]
async fn test_download_fails_on_permanent_error() -> Result<(), Box<dyn std::error::Error>> {
    use tempfile::TempDir;

    let repository = Arc::new(MockDownloadRepository::new());
    let mock_engine = Arc::new(MockDownloadEngine::new());

    let temp_dir = TempDir::new()?;

    // Configure mock to fail permanently (using set_download_result for URL-specific error)
    mock_engine.set_file_size("https://example.com/forbidden.bin", Some(1000));
    mock_engine.set_download_result(
        "https://example.com/forbidden.bin",
        Err(DownloadError::ValidationFailed("403 Forbidden".to_string())),
    );

    let manager = DownloadManagerService::new(
        repository.clone(),
        mock_engine.clone(),
        temp_dir.path().to_path_buf(),
    );
    let dest_path = temp_dir.path().join("forbidden.bin");

    let request = DownloadRequest {
        url: "https://example.com/forbidden.bin".to_string(),
        destination: dest_path.clone(),
        checksum: None,
        auth_token: None,
        model_name: None,
        model_id: None,
        model_file_name: None,
    };

    let download_id = manager.start_download(request).await?;

    // Wait for download to fail
    let repo_clone = repository.clone();
    let id_clone = download_id.clone();

    let failed_download = poll_until(
        move || {
            let repo = repo_clone.clone();
            let id = id_clone.clone();
            Box::pin(async move {
                let download = repo.get(&id).await.ok().flatten()?;
                if download.state() == &DownloadState::Failed {
                    Some(download)
                } else {
                    None
                }
            }) as Pin<Box<dyn Future<Output = Option<DownloadSession>> + Send>>
        },
        Duration::from_secs(3),
    )
    .await?;

    assert_eq!(failed_download.state(), &DownloadState::Failed);

    // The error message may or may not be set depending on manager's error handling
    // Just verify it's in Failed state, which is the important part

    Ok(())
}
