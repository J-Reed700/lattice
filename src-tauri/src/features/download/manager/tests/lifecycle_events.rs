//! Event-channel backpressure must not wedge transfer lifecycle operations.

use super::{temp_file, temp_root};
use crate::domain::download::{DownloadSession, DownloadState};
use crate::features::download::download_repository::mock::MockDownloadRepository;
use crate::features::download::download_repository::DownloadRepository;
use crate::features::download::engine::mock::MockDownloadEngine;
use crate::features::download::manager::{
    DownloadEvent, DownloadManager, DownloadManagerService, DownloadRequest,
};
use std::sync::Arc;
use tokio::time::{timeout, Duration};

fn request(url: &str, name: &str, token: &str) -> DownloadRequest {
    DownloadRequest {
        url: url.to_string(),
        destination: temp_file(name),
        checksum: None,
        auth_token: Some(token.to_string()),
        model_name: None,
        model_id: None,
        model_file_name: None,
    }
}

async fn stall_event_receiver(
    manager: &DownloadManagerService,
) -> tokio::sync::mpsc::Receiver<DownloadEvent> {
    let receiver = manager.subscribe_to_events();
    let stalled_receiver = receiver.write().await.take().unwrap();
    for index in 0..128 {
        assert!(manager
            .event_tx
            .try_send(DownloadEvent::Progress {
                id: format!("buffered-{index}"),
                bytes_downloaded: index,
                bytes_per_second: 0.0,
            })
            .is_ok());
    }
    stalled_receiver
}

#[tokio::test]
async fn full_event_channel_does_not_block_pause_or_cancel_and_cleans_registry() {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    engine.set_stall(true);
    let manager = DownloadManagerService::new(repository.clone(), engine.clone(), temp_root());
    let _stalled_receiver = stall_event_receiver(&manager).await;

    let pause_url = "https://example.com/backpressured-pause.bin";
    engine.set_file_size(pause_url, Some(1000));
    let pause_id = manager
        .start_download(request(
            pause_url,
            "backpressured-pause.bin",
            "pause-secret",
        ))
        .await
        .unwrap();
    assert!(manager
        .active_downloads
        .read()
        .await
        .contains_key(&pause_id));

    timeout(Duration::from_secs(1), manager.pause_download(&pause_id))
        .await
        .expect("pause must not wait forever for the full event channel")
        .unwrap();
    assert_eq!(
        repository.get(&pause_id).await.unwrap().unwrap().state(),
        &DownloadState::Paused
    );
    assert!(!manager
        .active_downloads
        .read()
        .await
        .contains_key(&pause_id));
    assert!(!manager.auth_tokens.read().await.contains_key(&pause_id));

    let cancel_url = "https://example.com/backpressured-cancel.bin";
    engine.set_file_size(cancel_url, Some(1000));
    let cancel_id = manager
        .start_download(request(
            cancel_url,
            "backpressured-cancel.bin",
            "cancel-secret",
        ))
        .await
        .unwrap();

    timeout(Duration::from_secs(1), manager.cancel_download(&cancel_id))
        .await
        .expect("cancel must not wait forever for the full event channel")
        .unwrap();
    assert_eq!(
        repository.get(&cancel_id).await.unwrap().unwrap().state(),
        &DownloadState::Cancelled
    );
    assert!(!manager
        .active_downloads
        .read()
        .await
        .contains_key(&cancel_id));
    assert!(!manager.auth_tokens.read().await.contains_key(&cancel_id));
}

#[tokio::test]
async fn root_cancellation_cleans_active_download_with_full_event_channel() {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    engine.set_stall(true);
    let manager = DownloadManagerService::new(repository.clone(), engine.clone(), temp_root());
    let _stalled_receiver = stall_event_receiver(&manager).await;

    let url = "https://example.com/shutdown-backpressure.bin";
    engine.set_file_size(url, Some(1000));
    let id = manager
        .start_download(request(url, "shutdown-backpressure.bin", "shutdown-secret"))
        .await
        .unwrap();
    assert!(manager.active_downloads.read().await.contains_key(&id));

    manager.shutdown.cancel();

    timeout(Duration::from_secs(1), async {
        loop {
            if !manager.active_downloads.read().await.contains_key(&id) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("shutdown must release the transfer even when its event channel is full");
    assert!(!manager.auth_tokens.read().await.contains_key(&id));
    assert_eq!(
        repository.get(&id).await.unwrap().unwrap().state(),
        &DownloadState::Paused
    );
}

#[tokio::test]
async fn full_event_channel_drops_terminal_hint_but_keeps_reconciliation_candidate() {
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    let manager = DownloadManagerService::new(repository.clone(), engine.clone(), temp_root());
    let _stalled_receiver = stall_event_receiver(&manager).await;

    let url = "https://example.com/terminal-backpressure.bin";
    engine.set_file_size(url, Some(1000));
    let id = manager
        .start_download(request(url, "terminal-backpressure.bin", "terminal-secret"))
        .await
        .unwrap();

    timeout(Duration::from_secs(1), async {
        loop {
            let session = repository.get(&id).await.unwrap().unwrap();
            if session.state() == &DownloadState::Failed
                && !manager.active_downloads.read().await.contains_key(&id)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("durable failure and registry cleanup must not wait on UI delivery");
    assert!(!manager.auth_tokens.read().await.contains_key(&id));

    let recovery_page = repository
        .list_terminal_sessions_needing_reconciliation(None, 64)
        .await
        .unwrap();
    assert!(recovery_page.iter().any(|session| session.id() == id));
}

#[tokio::test]
async fn terminal_reconciliation_pages_are_bounded_and_keyset_ordered() {
    let repository = MockDownloadRepository::new();
    for index in 0..70 {
        let mut session = DownloadSession::new(
            format!("terminal-{index:03}"),
            format!("https://example.com/{index}"),
            temp_file(&format!("page-{index}.bin")),
            Some(1),
            None,
        )
        .unwrap();
        session.start().unwrap();
        session.fail("test failure".to_string()).unwrap();
        repository.create(&session).await.unwrap();
    }

    let first = repository
        .list_terminal_sessions_needing_reconciliation(None, 1000)
        .await
        .unwrap();
    assert_eq!(first.len(), 64, "page size is capped at 64");
    let second = repository
        .list_terminal_sessions_needing_reconciliation(first.last().map(|session| session.id()), 64)
        .await
        .unwrap();
    assert_eq!(second.len(), 6);
    assert!(first.last().unwrap().id() < second.first().unwrap().id());
}
