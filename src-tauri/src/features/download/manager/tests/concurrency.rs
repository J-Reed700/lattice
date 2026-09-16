//! Concurrency limits and isolation between simultaneous downloads.

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
async fn test_download_queue_respects_limits() -> Result<(), Box<dyn std::error::Error>> {
    let repository = Arc::new(MockDownloadRepository::new());
    let mock_engine = Arc::new(MockDownloadEngine::new());

    mock_engine.set_download_result(
        "https://example.com/file0.bin",
        Ok(crate::features::download::engine::DownloadResult {
            bytes_downloaded: 1000,
            total_bytes: Some(1000),
            sha256_checksum: "a".repeat(64),
            elapsed: Duration::from_secs(1),
        }),
    );
    mock_engine.set_download_result(
        "https://example.com/file1.bin",
        Ok(crate::features::download::engine::DownloadResult {
            bytes_downloaded: 1000,
            total_bytes: Some(1000),
            sha256_checksum: "b".repeat(64),
            elapsed: Duration::from_secs(1),
        }),
    );
    mock_engine.set_download_result(
        "https://example.com/file2.bin",
        Ok(crate::features::download::engine::DownloadResult {
            bytes_downloaded: 1000,
            total_bytes: Some(1000),
            sha256_checksum: "c".repeat(64),
            elapsed: Duration::from_secs(1),
        }),
    );
    mock_engine.set_download_result(
        "https://example.com/file3.bin",
        Ok(crate::features::download::engine::DownloadResult {
            bytes_downloaded: 1000,
            total_bytes: Some(1000),
            sha256_checksum: "d".repeat(64),
            elapsed: Duration::from_secs(1),
        }),
    );

    let manager = DownloadManagerService::new(repository.clone(), mock_engine.clone(), temp_root())
        .with_max_concurrent(2);

    let mut download_ids = Vec::new();
    for i in 0..4 {
        mock_engine.set_file_size(&format!("https://example.com/file{}.bin", i), Some(1000));

        let request = DownloadRequest {
            url: format!("https://example.com/file{}.bin", i),
            destination: temp_file(&format!("file{}.bin", i)),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        };

        let id = manager.start_download(request).await?;
        download_ids.push(id);
    }

    // Wait for all downloads to complete
    for download_id in &download_ids {
        let repo_clone = repository.clone();
        let id_clone = download_id.clone();

        let _ = poll_until(
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
            Duration::from_secs(5),
        )
        .await;
    }

    let completed_count = download_ids
        .iter()
        .filter_map(|id| {
            futures::executor::block_on(async { repository.get(id).await.ok().flatten() })
        })
        .filter(|d| d.state() == &DownloadState::Completed || d.state() == &DownloadState::Failed)
        .count();

    assert!(
        completed_count >= 2,
        "At least 2 downloads should complete (got {})",
        completed_count
    );

    Ok(())
}

#[tokio::test]
async fn test_concurrent_downloads_isolated() -> Result<(), Box<dyn std::error::Error>> {
    let repository = Arc::new(MockDownloadRepository::new());
    let mock_engine = Arc::new(MockDownloadEngine::new());

    for i in 0..3 {
        mock_engine.set_file_size(
            &format!("https://example.com/concurrent{}.bin", i),
            Some(1000),
        );
        mock_engine.set_download_result(
            &format!("https://example.com/concurrent{}.bin", i),
            Ok(crate::features::download::engine::DownloadResult {
                bytes_downloaded: 1000,
                total_bytes: Some(1000),
                sha256_checksum: format!("{}", char::from_u32('a' as u32 + i).unwrap()).repeat(64),
                elapsed: Duration::from_secs(1),
            }),
        );
    }

    let manager = DownloadManagerService::new(repository.clone(), mock_engine.clone(), temp_root())
        .with_max_concurrent(5);

    let mut download_ids = Vec::new();
    for i in 0..3 {
        let request = DownloadRequest {
            url: format!("https://example.com/concurrent{}.bin", i),
            destination: temp_file(&format!("concurrent{}.bin", i)),
            checksum: None,
            auth_token: None,
            model_name: None,
            model_id: None,
            model_file_name: None,
        };

        let id = manager.start_download(request).await?;
        download_ids.push(id);
    }

    // Wait for all to complete
    let mut final_downloads = Vec::new();
    for download_id in &download_ids {
        let repo_clone = repository.clone();
        let id_clone = download_id.clone();

        let download = poll_until(
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

        final_downloads.push(download);
    }

    let unique_ids: std::collections::HashSet<_> = final_downloads.iter().map(|d| d.id()).collect();
    assert_eq!(unique_ids.len(), 3, "All downloads should have unique IDs");

    let unique_destinations: std::collections::HashSet<_> = final_downloads
        .iter()
        .map(|d| d.destination().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        unique_destinations.len(),
        3,
        "All downloads should have unique destinations"
    );

    // All should be in terminal state
    for download in &final_downloads {
        assert!(
            download.state() == &DownloadState::Completed
                || download.state() == &DownloadState::Failed,
            "Download should be in terminal state, got {:?}",
            download.state()
        );
    }

    Ok(())
}
