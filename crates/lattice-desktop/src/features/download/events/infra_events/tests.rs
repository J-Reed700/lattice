//! Tests for queue promotion, session file names and reconciliation polling.

use super::{
    is_terminal_manager_event, model_file_name_for_session, process_queue_after_terminal,
    reconciliation_poll_delay, should_emit_reconciled_failed_alert, RECONCILIATION_ACTIVE_POLL,
    RECONCILIATION_BATCH_SIZE, RECONCILIATION_IDLE_POLL,
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
        _auth_token: Option<&str>,
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

        std::fs::write(dir.path().join("first.bin.part"), vec![1_u8; 1000]).unwrap();
        std::fs::write(dir.path().join("queued.bin.part"), vec![1_u8; 1000]).unwrap();

        let first_id = manager
            .start_download(DownloadRequest {
                url: first_url.into(),
                destination: dir.path().join("first.bin"),
                checksum: None,
                auth_token: None,
                model_name: None,
                model_id: None,
                model_file_name: None,
                expected_bytes: None,
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
                expected_bytes: None,
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

#[test]
fn reconciled_failed_alerts_are_suppressed_for_historical_sessions_only() {
    let startup = chrono::Utc::now();
    let historical = startup - chrono::Duration::seconds(5);
    let runtime = startup + chrono::Duration::seconds(1);
    assert!(!should_emit_reconciled_failed_alert(&historical, &startup));
    assert!(should_emit_reconciled_failed_alert(&runtime, &startup));
}

#[tokio::test]
async fn batch_snapshot_status_precedence_and_error_messages_are_accurate() {
    use lattice_core::domain::download::snapshot::DownloadStatus;
    let repository = Arc::new(MockDownloadRepository::new());

    // Downloading + Paused -> Downloading (not Paused).
    let mut downloading = DownloadSession::new(
        "s-dl".into(),
        "https://example.com/a.bin".into(),
        std::path::PathBuf::from("/tmp/m1/a.bin"),
        Some(100),
        None,
    )
    .unwrap()
    .with_model_metadata("M1".into(), "m1".into())
    .with_model_file_name("a.bin".into());
    downloading.start().unwrap();
    let mut paused = DownloadSession::new(
        "s-paused".into(),
        "https://example.com/b.bin".into(),
        std::path::PathBuf::from("/tmp/m1/b.bin"),
        Some(100),
        None,
    )
    .unwrap()
    .with_model_metadata("M1".into(), "m1".into())
    .with_model_file_name("b.bin".into());
    paused.pause().unwrap();
    repository.create(&downloading).await.unwrap();
    repository.create(&paused).await.unwrap();

    // Cancelled batch -> Cancelled (not Downloading).
    let mut c1 = DownloadSession::new(
        "c-1".into(),
        "https://example.com/c1.bin".into(),
        std::path::PathBuf::from("/tmp/m2/c1.bin"),
        Some(100),
        None,
    )
    .unwrap()
    .with_model_metadata("M2".into(), "m2".into())
    .with_model_file_name("c1.bin".into());
    c1.cancel().unwrap();
    let mut c2 = DownloadSession::new(
        "c-2".into(),
        "https://example.com/c2.bin".into(),
        std::path::PathBuf::from("/tmp/m2/c2.bin"),
        Some(100),
        None,
    )
    .unwrap()
    .with_model_metadata("M2".into(), "m2".into())
    .with_model_file_name("c2.bin".into());
    c2.cancel().unwrap();
    repository.create(&c1).await.unwrap();
    repository.create(&c2).await.unwrap();

    // Single failed session preserves model_id and error_message.
    let mut single_failed = DownloadSession::new(
        "s-single".into(),
        "https://example.com/single.gguf".into(),
        std::path::PathBuf::from("/tmp/single.gguf"),
        Some(100),
        None,
    )
    .unwrap()
    .with_model_metadata("Single".into(), "single-model".into());
    single_failed.start().unwrap();
    single_failed.fail("HTTP 401".into()).unwrap();
    repository.create(&single_failed).await.unwrap();

    // Verify status precedence and snapshot payload fields via snapshot helpers:
    let batch_downloading =
        super::snapshots::build_batch_snapshot_from_sessions("m1", &[downloading, paused]).unwrap();
    assert!(matches!(
        batch_downloading.status,
        DownloadStatus::Downloading
    ));
    let batch_cancelled =
        super::snapshots::build_batch_snapshot_from_sessions("m2", &[c1, c2]).unwrap();
    assert!(matches!(batch_cancelled.status, DownloadStatus::Cancelled));

    let failed_snapshot = super::snapshots::build_single_failed_snapshot(
        "s-single".into(),
        &single_failed,
        "fallback".into(),
    );
    assert_eq!(failed_snapshot.model_id.as_deref(), Some("single-model"));
    assert_eq!(failed_snapshot.error_message.as_deref(), Some("HTTP 401"));
    assert!(matches!(failed_snapshot.status, DownloadStatus::Error));
}
