//! One live session per destination, sizing outside the queue gate, and names.

use crate::features::download::download_repository::mock::MockDownloadRepository;
use crate::features::download::download_repository::DownloadRepository;
use crate::features::download::engine::mock::MockDownloadEngine;
use crate::features::download::manager::DownloadManagerService;
use lattice_core::application::ports::download_manager::{
    DownloadBatchItem, DownloadManager, DownloadRequest,
};
use lattice_core::domain::download::{DownloadError, DownloadSession, DownloadState};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

fn request(url: &str, destination: &Path) -> DownloadRequest {
    DownloadRequest {
        url: url.to_string(),
        destination: destination.to_path_buf(),
        checksum: None,
        auth_token: None,
        model_name: None,
        model_id: None,
        model_file_name: None,
        expected_bytes: None,
    }
}

struct Fixture {
    dir: tempfile::TempDir,
    repository: Arc<MockDownloadRepository>,
    engine: Arc<MockDownloadEngine>,
    manager: Arc<DownloadManagerService>,
}

fn fixture(max_concurrent: usize) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = Arc::new(
        DownloadManagerService::new(
            repository.clone(),
            engine.clone(),
            dir.path().to_path_buf(),
            Arc::default(),
        )
        .with_max_concurrent(max_concurrent),
    );
    Fixture {
        dir,
        repository,
        engine,
        manager,
    }
}

#[tokio::test]
async fn a_second_start_for_a_running_or_queued_destination_returns_the_live_id() {
    let f = fixture(1);
    f.engine.set_stall(true);
    let running_path = f.dir.path().join("running.bin");
    let queued_path = f.dir.path().join("queued.bin");

    let running = f
        .manager
        .start_download(request("https://example.com/running.bin", &running_path))
        .await
        .unwrap();
    let queued = f
        .manager
        .start_download(request("https://example.com/queued.bin", &queued_path))
        .await
        .unwrap();
    assert_eq!(f.repository.list().await.unwrap().len(), 2);

    let running_again = f
        .manager
        .start_download(request("https://example.com/running.bin", &running_path))
        .await
        .unwrap();
    let queued_again = f
        .manager
        .start_download(request("https://example.com/queued.bin", &queued_path))
        .await
        .unwrap();

    assert_eq!(running_again, running);
    assert_eq!(queued_again, queued);
    assert_eq!(
        f.repository.list().await.unwrap().len(),
        2,
        "nothing new was registered"
    );
}

#[tokio::test]
async fn a_batch_returns_one_id_per_item_and_coalesces_shared_destinations() {
    let f = fixture(2);
    f.engine.set_stall(true);
    let shared = f.dir.path().join("shared.bin");
    let other = f.dir.path().join("other.bin");

    let ids = f
        .manager
        .start_download_batch(vec![
            DownloadBatchItem {
                requests: vec![request("https://example.com/shared.bin", &shared)],
            },
            DownloadBatchItem {
                requests: vec![request("https://example.com/other.bin", &other)],
            },
            DownloadBatchItem {
                requests: vec![request("https://example.com/shared.bin", &shared)],
            },
        ])
        .await
        .unwrap();

    assert_eq!(ids.len(), 3);
    assert_eq!(ids[0], ids[2]);
    assert_ne!(ids[0], ids[1]);
    assert_eq!(f.repository.list().await.unwrap().len(), 2);

    let again = f
        .manager
        .start_download_batch(vec![DownloadBatchItem {
            requests: vec![request("https://example.com/shared.bin", &shared)],
        }])
        .await
        .unwrap();
    assert_eq!(again, vec![ids[0].clone()]);
    assert_eq!(f.repository.list().await.unwrap().len(), 2);
}

#[tokio::test]
async fn sessions_that_are_not_live_are_superseded() {
    let f = fixture(1);
    f.engine.set_stall(true);
    let destination = f.dir.path().join("file.bin");
    let mut failed = DownloadSession::new(
        "failed-earlier".into(),
        "https://example.com/file.bin".into(),
        destination.clone(),
        Some(1000),
        None,
    )
    .unwrap();
    failed.start().unwrap();
    failed.fail("boom".into()).unwrap();
    f.repository.create(&failed).await.unwrap();

    let id = f
        .manager
        .start_download(request("https://example.com/file.bin", &destination))
        .await
        .unwrap();

    assert_ne!(id, "failed-earlier");
    assert!(f.repository.get("failed-earlier").await.unwrap().is_none());
}

#[tokio::test]
async fn retry_and_resume_refuse_while_another_session_for_the_destination_is_live() {
    let f = fixture(1);
    f.engine.set_stall(true);
    let destination = f.dir.path().join("file.bin");
    f.manager
        .start_download(request("https://example.com/file.bin", &destination))
        .await
        .unwrap();

    let new_session = |id: &str| {
        DownloadSession::new(
            id.into(),
            "https://example.com/file.bin".into(),
            destination.clone(),
            Some(1000),
            None,
        )
        .unwrap()
    };
    let mut failed = new_session("failed");
    failed.start().unwrap();
    failed.fail("boom".into()).unwrap();
    f.repository.create(&failed).await.unwrap();
    let mut paused = new_session("paused");
    paused.start().unwrap();
    paused.pause().unwrap();
    f.repository.create(&paused).await.unwrap();

    let retry = f.manager.retry_download("failed").await.unwrap_err();
    let resume = f.manager.resume_download("paused").await.unwrap_err();

    assert!(matches!(retry, DownloadError::DestinationBusy(_)));
    assert!(matches!(resume, DownloadError::DestinationBusy(_)));
    assert_eq!(
        f.repository.get("failed").await.unwrap().unwrap().state(),
        &DownloadState::Failed
    );
    assert_eq!(
        f.repository.get("paused").await.unwrap().unwrap().state(),
        &DownloadState::Paused
    );
}

#[tokio::test]
async fn a_known_size_means_no_head_request_is_sent() {
    let f = fixture(1);
    f.engine.set_stall(true);
    let mut pinned = request(
        "https://example.com/pinned.bin",
        &f.dir.path().join("p.bin"),
    );
    pinned.expected_bytes = Some(639);

    let id = f.manager.start_download(pinned).await.unwrap();

    assert_eq!(f.engine.file_size_calls(), 0);
    let session = f.repository.get(&id).await.unwrap().unwrap();
    assert_eq!(session.progress().total_bytes(), Some(639));
}

#[tokio::test]
async fn a_slow_size_probe_does_not_block_pause_or_cancel_of_another_download() {
    let f = fixture(2);
    f.engine.set_stall(true);
    let mut known = request("https://example.com/a.bin", &f.dir.path().join("a.bin"));
    known.expected_bytes = Some(1000);
    let first = f.manager.start_download(known).await.unwrap();
    let mut other = request("https://example.com/c.bin", &f.dir.path().join("c.bin"));
    other.expected_bytes = Some(1000);
    let second = f.manager.start_download(other).await.unwrap();

    let release = f.engine.hold_file_size();
    let slow = {
        let manager = f.manager.clone();
        let destination = f.dir.path().join("slow.bin");
        tokio::spawn(async move {
            manager
                .start_download(request("https://example.com/slow.bin", &destination))
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;

    tokio::time::timeout(Duration::from_secs(2), f.manager.pause_download(&first))
        .await
        .expect("pause waited on a slow HEAD")
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), f.manager.cancel_download(&second))
        .await
        .expect("cancel waited on a slow HEAD")
        .unwrap();

    release.notify_one();
    let started = slow.await.unwrap().unwrap();
    assert!(!started.is_empty());
}

#[tokio::test]
async fn a_name_is_kept_without_a_model_id() {
    let f = fixture(1);
    f.engine.set_stall(true);
    let mut named = request("https://example.com/n.bin", &f.dir.path().join("n.bin"));
    named.model_name = Some("Vision projector".into());

    let id = f.manager.start_download(named).await.unwrap();

    let session = f.repository.get(&id).await.unwrap().unwrap();
    assert_eq!(session.model_name(), Some("Vision projector"));
    assert_eq!(session.model_id(), None);
}

#[tokio::test]
async fn a_queued_download_can_be_paused_and_leaves_the_queue() {
    let f = fixture(1);
    f.engine.set_stall(true);
    let first = f
        .manager
        .start_download(request(
            "https://example.com/1.bin",
            &f.dir.path().join("1.bin"),
        ))
        .await
        .unwrap();
    let queued = f
        .manager
        .start_download(request(
            "https://example.com/2.bin",
            &f.dir.path().join("2.bin"),
        ))
        .await
        .unwrap();
    assert_eq!(
        f.repository.get(&queued).await.unwrap().unwrap().state(),
        &DownloadState::Pending
    );

    f.manager.pause_download(&queued).await.unwrap();

    assert_eq!(
        f.repository.get(&queued).await.unwrap().unwrap().state(),
        &DownloadState::Paused
    );
    assert!(!f.manager.download_queue.read().await.contains(&queued));
    assert!(f.manager.active_downloads.read().await.contains_key(&first));
}

#[tokio::test]
async fn a_finished_task_lingering_in_the_active_table_is_not_live() {
    use crate::features::download::manager::state::ActiveDownload;
    let f = fixture(1);
    f.engine.set_stall(true);
    let destination = f.dir.path().join("done.bin");
    let mut done = DownloadSession::new(
        "done".into(),
        "https://example.com/done.bin".into(),
        destination.clone(),
        Some(1000),
        None,
    )
    .unwrap();
    done.start().unwrap();
    done.complete().unwrap();
    f.repository.create(&done).await.unwrap();
    f.manager.active_downloads.write().await.insert(
        "done".into(),
        ActiveDownload {
            task_handle: None,
            cancel_tx: None,
        },
    );

    let id = f
        .manager
        .start_download(request("https://example.com/done.bin", &destination))
        .await
        .unwrap();

    assert_ne!(
        id, "done",
        "a finished session is superseded, not coalesced into"
    );
}

#[tokio::test]
async fn an_expected_size_is_not_trusted_when_a_request_has_fallback_urls() {
    let f = fixture(1);
    f.engine.set_stall(true);
    let mut primary = request("https://example.com/p.bin", &f.dir.path().join("p.bin"));
    primary.expected_bytes = Some(1000);
    let fallback = request("https://example.com/q.bin", &f.dir.path().join("p.bin"));

    f.manager
        .start_download_batch(vec![DownloadBatchItem {
            requests: vec![primary, fallback],
        }])
        .await
        .unwrap();

    assert_eq!(
        f.engine.file_size_calls(),
        1,
        "the HEAD still decides the 404 fallback"
    );
}

#[tokio::test]
async fn resuming_while_both_slots_are_busy_leaves_the_session_pending_until_a_slot_frees() {
    let f = fixture(1);
    f.engine.set_stall(true);
    let busy = f
        .manager
        .start_download(request(
            "https://example.com/1.bin",
            &f.dir.path().join("1.bin"),
        ))
        .await
        .unwrap();
    let mut paused = DownloadSession::new(
        "paused".into(),
        "https://example.com/2.bin".into(),
        f.dir.path().join("2.bin"),
        Some(1000),
        None,
    )
    .unwrap();
    paused.start().unwrap();
    paused.pause().unwrap();
    f.repository.create(&paused).await.unwrap();

    f.manager.resume_download("paused").await.unwrap();

    assert_eq!(
        f.repository.get("paused").await.unwrap().unwrap().state(),
        &DownloadState::Pending
    );
    assert!(f
        .manager
        .download_queue
        .read()
        .await
        .contains(&"paused".to_string()));

    f.manager.pause_download(&busy).await.unwrap();
    f.manager.process_pending_queue().await.unwrap();

    assert_eq!(
        f.repository.get("paused").await.unwrap().unwrap().state(),
        &DownloadState::Downloading
    );
}
