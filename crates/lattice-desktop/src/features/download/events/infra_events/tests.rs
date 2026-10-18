//! Tests for queue promotion, session file names and reconciliation polling.

use super::{
    is_terminal_manager_event, model_file_name_for_session, process_queue_after_terminal,
    reconciliation_poll_delay, RECONCILIATION_ACTIVE_POLL, RECONCILIATION_BATCH_SIZE,
    RECONCILIATION_IDLE_POLL,
};
use crate::features::download::download_repository::mock::MockDownloadRepository;
use crate::features::download::download_repository::DownloadRepository;
use crate::features::download::manager::DownloadManagerService;
use lattice_core::application::ports::download_manager::{
    DownloadEvent, DownloadManager, DownloadRequest,
};
use lattice_core::domain::download::DownloadSession;
use std::sync::Arc;
use std::time::Duration;

struct GatedEngine {
    first_url: String,
    first_fails: bool,
    first_started: tokio::sync::Notify,
    release_first: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl crate::features::download::engine::DownloadEngine for GatedEngine {
    async fn download(
        &self,
        options: crate::features::download::engine::DownloadOptions,
    ) -> Result<
        crate::features::download::engine::DownloadResult,
        lattice_core::domain::download::DownloadError,
    > {
        if options.url == self.first_url {
            self.first_started.notify_one();
            self.release_first.notified().await;
            if self.first_fails {
                return Err(lattice_core::domain::download::DownloadError::NetworkError(
                    "test failure".into(),
                ));
            }
        }
        Ok(crate::features::download::engine::DownloadResult {
            bytes_downloaded: 1000,
            total_bytes: Some(1000),
            sha256_checksum: "a".repeat(64),
            elapsed: Duration::from_millis(1),
        })
    }

    async fn get_file_size(
        &self,
        url: &str,
    ) -> Result<(Option<u64>, String), lattice_core::domain::download::DownloadError> {
        Ok((Some(1000), url.to_string()))
    }

    async fn supports_resume(
        &self,
        _url: &str,
    ) -> Result<bool, lattice_core::domain::download::DownloadError> {
        Ok(true)
    }
}

#[test]
fn queue_promotion_is_requested_after_terminal_events_only() {
    use lattice_core::application::ports::download_manager::DownloadEvent;
    assert!(super::is_terminal_manager_event(
        &DownloadEvent::Completed { id: "done".into() }
    ));
    assert!(super::is_terminal_manager_event(&DownloadEvent::Failed {
        id: "failed".into(),
        error: "x".into()
    }));
    assert!(super::is_terminal_manager_event(
        &DownloadEvent::Cancelled {
            id: "cancelled".into()
        }
    ));
    assert!(!super::is_terminal_manager_event(
        &DownloadEvent::Progress {
            id: "active".into(),
            bytes_downloaded: 1,
            bytes_per_second: 1.0
        }
    ));
}

#[tokio::test]
async fn terminal_failure_and_completion_each_promote_the_next_queued_download() {
    for first_fails in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let repository = Arc::new(MockDownloadRepository::new());
        let first_url = if first_fails {
            "https://example.test/failure"
        } else {
            "https://example.test/completion"
        };
        let second_url = "https://example.test/queued";
        let engine = Arc::new(GatedEngine {
            first_url: first_url.into(),
            first_fails,
            first_started: tokio::sync::Notify::new(),
            release_first: tokio::sync::Notify::new(),
        });
        let manager = DownloadManagerService::new(
            repository.clone(),
            engine.clone(),
            dir.path().to_path_buf(),
            Arc::default(),
        )
        .with_max_concurrent(1);
        let mut events = manager.subscribe_to_events().write().await.take().unwrap();

        std::fs::write(dir.path().join("first.bin"), vec![1_u8; 1000]).unwrap();
        std::fs::write(dir.path().join("queued.bin"), vec![1_u8; 1000]).unwrap();

        let first_id = manager
            .start_download(DownloadRequest {
                url: first_url.into(),
                destination: dir.path().join("first.bin"),
                checksum: None,
                auth_token: None,
                model_name: None,
                model_id: None,
                model_file_name: None,
            })
            .await
            .unwrap();
        engine.first_started.notified().await;
        let queued_id = manager
            .start_download(DownloadRequest {
                url: second_url.into(),
                destination: dir.path().join("queued.bin"),
                checksum: None,
                auth_token: None,
                model_name: None,
                model_id: None,
                model_file_name: None,
            })
            .await
            .unwrap();
        engine.release_first.notify_one();

        let terminal = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let event = events.recv().await.expect("manager event channel closed");
                let terminal_for_first = matches!(
                    &event,
                    DownloadEvent::Completed { id } | DownloadEvent::Failed { id, .. } if id == &first_id
                );
                if terminal_for_first { break event; }
            }
        }).await.expect("first download should finish");
        process_queue_after_terminal(&manager, is_terminal_manager_event(&terminal))
            .await
            .unwrap();

        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if repository
                    .get(&queued_id)
                    .await
                    .unwrap()
                    .is_some_and(|session| {
                        session.state() != &lattice_core::domain::download::DownloadState::Pending
                    })
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("terminal event should promote queued download");
    }
}

#[test]
fn session_preserves_manifest_relative_file_name() {
    let session = DownloadSession::new(
        "download-id".to_string(),
        "https://example.test/onnx/model.onnx".to_string(),
        std::path::PathBuf::from("/home/u/.cache/lattice/models/all-mpnet-base-v2/model.onnx"),
        Some(42),
        None,
    )
    .expect("session")
    .with_model_file_name("onnx/model.onnx".to_string());

    assert_eq!(model_file_name_for_session(&session), "onnx/model.onnx");
}

#[test]
fn reconciliation_poll_slows_when_idle_or_after_an_error_but_keeps_paging_backlog() {
    assert_eq!(
        reconciliation_poll_delay(0, false),
        RECONCILIATION_IDLE_POLL
    );
    assert_eq!(
        reconciliation_poll_delay(RECONCILIATION_BATCH_SIZE - 1, false),
        RECONCILIATION_IDLE_POLL
    );
    assert_eq!(reconciliation_poll_delay(0, true), RECONCILIATION_IDLE_POLL);
    assert_eq!(
        reconciliation_poll_delay(RECONCILIATION_BATCH_SIZE, false),
        RECONCILIATION_ACTIVE_POLL
    );
}
