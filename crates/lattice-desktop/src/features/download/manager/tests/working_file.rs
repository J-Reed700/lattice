//! The `.part` working file: written during the transfer, renamed onto the
//! destination only after size and checksum pass.

use super::poll_until;
use crate::features::download::download_repository::mock::MockDownloadRepository;
use crate::features::download::download_repository::DownloadRepository;
use crate::features::download::engine::mock::MockDownloadEngine;
use crate::features::download::manager::DownloadManagerService;
use lattice_core::application::ports::download_manager::{DownloadManager, DownloadRequest};
use lattice_core::domain::download::{
    part_path, Checksum, ChecksumAlgorithm, DownloadSession, DownloadState,
};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use tokio::time::Duration;

const URL: &str = "https://example.com/model.gguf";

struct Fixture {
    dir: tempfile::TempDir,
    repository: Arc<MockDownloadRepository>,
    engine: Arc<MockDownloadEngine>,
    manager: DownloadManagerService,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let repository = Arc::new(MockDownloadRepository::new());
    let engine = Arc::new(MockDownloadEngine::new());
    engine.set_file_size(URL, Some(1000));
    let manager = DownloadManagerService::new(
        repository.clone(),
        engine.clone(),
        dir.path().to_path_buf(),
        Arc::default(),
    );
    Fixture {
        dir,
        repository,
        engine,
        manager,
    }
}

fn checksum(c: char) -> Checksum {
    Checksum::new(ChecksumAlgorithm::Sha256, c.to_string().repeat(64)).unwrap()
}

fn request(destination: &Path, checksum: Option<Checksum>) -> DownloadRequest {
    DownloadRequest {
        url: URL.to_string(),
        destination: destination.to_path_buf(),
        checksum,
        auth_token: None,
        model_name: None,
        model_id: None,
        model_file_name: None,
        expected_bytes: None,
    }
}

async fn settled(f: &Fixture, id: &str) -> DownloadSession {
    let repository = f.repository.clone();
    let id = id.to_string();
    poll_until(
        move || {
            let repository = repository.clone();
            let id = id.clone();
            Box::pin(async move {
                let session = repository.get(&id).await.ok().flatten()?;
                session.state().is_terminal().then_some(session)
            }) as Pin<Box<dyn Future<Output = Option<DownloadSession>> + Send>>
        },
        Duration::from_secs(5),
    )
    .await
    .unwrap()
}

async fn first_call(f: &Fixture) -> crate::features::download::engine::DownloadOptions {
    let engine = f.engine.clone();
    poll_until(
        move || {
            let engine = engine.clone();
            Box::pin(async move { engine.get_last_call() })
                as Pin<
                    Box<
                        dyn Future<
                                Output = Option<crate::features::download::engine::DownloadOptions>,
                            > + Send,
                    >,
                >
        },
        Duration::from_secs(5),
    )
    .await
    .unwrap()
}

async fn file_appears(path: PathBuf) {
    poll_until(
        move || {
            let path = path.clone();
            Box::pin(async move { path.exists().then_some(()) })
                as Pin<Box<dyn Future<Output = Option<()>> + Send>>
        },
        Duration::from_secs(5),
    )
    .await
    .unwrap();
}

fn failed_session(id: &str, destination: &Path, checksum: Option<Checksum>) -> DownloadSession {
    let mut session = DownloadSession::new(
        id.into(),
        URL.into(),
        destination.to_path_buf(),
        Some(1000),
        checksum,
    )
    .unwrap();
    session.start().unwrap();
    session.fail("Network error".into()).unwrap();
    session
}

#[tokio::test]
async fn the_final_path_is_absent_until_verification_passes() {
    let f = fixture();
    f.engine.set_write_len(Some(1000));
    f.engine.set_stall(true);
    let destination = f.dir.path().join("model.gguf");

    let id = f
        .manager
        .start_download(request(&destination, None))
        .await
        .unwrap();
    file_appears(part_path(&destination)).await;
    assert!(
        !destination.exists(),
        "a transfer in flight writes the .part"
    );

    f.manager.pause_download(&id).await.unwrap();
    f.engine.set_stall(false);
    f.manager.resume_download(&id).await.unwrap();
    let session = settled(&f, &id).await;

    assert_eq!(session.state(), &DownloadState::Completed);
    assert_eq!(std::fs::metadata(&destination).unwrap().len(), 1000);
    assert!(!part_path(&destination).exists());
}

#[tokio::test]
async fn a_checksum_failure_deletes_the_part_and_leaves_the_final_file_alone() {
    let f = fixture();
    f.engine.set_write_len(Some(1000));
    let destination = f.dir.path().join("model.gguf");
    std::fs::write(&destination, vec![1u8; 1200]).unwrap();

    // A final file of another length is neither complete nor a partial.
    // The mock reports a digest of 'a's; the pinned checksum is all 'b's.
    let id = f
        .manager
        .start_download(request(&destination, Some(checksum('b'))))
        .await
        .unwrap();
    let session = settled(&f, &id).await;

    assert_eq!(session.state(), &DownloadState::Failed);
    assert!(!part_path(&destination).exists());
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        vec![1u8; 1200],
        "the earlier final file is untouched"
    );
    let message = session.error_message().unwrap();
    assert_eq!(
        message,
        "The downloaded file didn't match its published checksum, so it was discarded. Try again."
    );
}

#[tokio::test]
async fn a_checksum_failure_with_no_earlier_file_leaves_no_final_file() {
    let f = fixture();
    f.engine.set_write_len(Some(1000));
    let destination = f.dir.path().join("model.gguf");

    let id = f
        .manager
        .start_download(request(&destination, Some(checksum('b'))))
        .await
        .unwrap();
    settled(&f, &id).await;

    assert!(!destination.exists());
    assert!(!part_path(&destination).exists());
}

#[tokio::test]
async fn a_size_failure_discards_the_part_with_a_plain_message() {
    let f = fixture();
    f.engine.set_write_len(Some(600));
    let destination = f.dir.path().join("model.gguf");

    let id = f
        .manager
        .start_download(request(&destination, None))
        .await
        .unwrap();
    let session = settled(&f, &id).await;

    assert_eq!(session.state(), &DownloadState::Failed);
    assert!(!destination.exists());
    assert!(!part_path(&destination).exists());
    assert_eq!(
        session.error_message().unwrap(),
        "The downloaded file was 600 B instead of the expected 1.0 KB, so it was discarded. Try again."
    );
}

#[tokio::test]
async fn cancel_deletes_the_part() {
    let f = fixture();
    f.engine.set_write_len(Some(1000));
    f.engine.set_stall(true);
    let destination = f.dir.path().join("model.gguf");

    let id = f
        .manager
        .start_download(request(&destination, None))
        .await
        .unwrap();
    file_appears(part_path(&destination)).await;

    f.manager.cancel_download(&id).await.unwrap();

    assert!(!part_path(&destination).exists());
    assert!(!destination.exists());
}

#[tokio::test]
async fn retry_keeps_the_part_with_a_checksum_and_deletes_it_without_one() {
    let f = fixture();
    f.engine.set_stall(true);
    for (name, sum) in [("with.gguf", Some(checksum('a'))), ("without.gguf", None)] {
        let destination = f.dir.path().join(name);
        std::fs::write(part_path(&destination), vec![1u8; 300]).unwrap();
        let id = format!("failed-{name}");
        f.repository
            .create(&failed_session(&id, &destination, sum.clone()))
            .await
            .unwrap();

        f.manager.retry_download(&id).await.unwrap();

        assert_eq!(
            part_path(&destination).exists(),
            sum.is_some(),
            "{name}: .part kept only when a checksum can vouch for it"
        );
    }
}

#[tokio::test]
async fn a_partial_file_written_in_place_by_the_old_writer_migrates_and_resumes() {
    let f = fixture();
    f.engine.set_stall(true);
    let destination = f.dir.path().join("model.gguf");
    std::fs::write(&destination, vec![1u8; 500]).unwrap();

    f.manager
        .start_download(request(&destination, Some(checksum('a'))))
        .await
        .unwrap();
    let call = first_call(&f).await;

    assert_eq!(call.resume_from, Some(500));
    assert_eq!(call.destination, part_path(&destination));
    assert!(!destination.exists());
    assert_eq!(
        std::fs::metadata(part_path(&destination)).unwrap().len(),
        500
    );
}

#[tokio::test]
async fn a_full_size_final_file_with_a_bad_checksum_is_deleted_and_downloaded_again() {
    let f = fixture();
    f.engine.set_stall(true);
    let destination = f.dir.path().join("model.gguf");
    std::fs::write(&destination, vec![1u8; 1000]).unwrap();

    f.manager
        .start_download(request(&destination, Some(checksum('a'))))
        .await
        .unwrap();
    let call = first_call(&f).await;

    assert_eq!(call.resume_from, None);
    assert_eq!(call.known_total, Some(1000));
    assert_eq!(call.destination, part_path(&destination));
    assert!(!destination.exists(), "the mismatching file was deleted");
}

#[tokio::test]
async fn a_complete_part_on_start_is_verified_and_renamed_without_a_size_request() {
    let f = fixture();
    let destination = f.dir.path().join("model.gguf");
    std::fs::write(part_path(&destination), vec![1u8; 1000]).unwrap();
    let mut pinned = request(&destination, Some(checksum('a')));
    pinned.expected_bytes = Some(1000);

    let id = f.manager.start_download(pinned).await.unwrap();
    let session = settled(&f, &id).await;

    assert_eq!(session.state(), &DownloadState::Completed);
    assert_eq!(f.engine.file_size_calls(), 0);
    assert_eq!(f.engine.get_last_call().unwrap().resume_from, Some(1000));
    assert_eq!(std::fs::metadata(&destination).unwrap().len(), 1000);
    assert!(!part_path(&destination).exists());
}

#[tokio::test]
async fn removing_a_failed_row_deletes_its_part_but_a_completed_row_keeps_its_file() {
    let f = fixture();
    let failed_destination = f.dir.path().join("failed.gguf");
    std::fs::write(part_path(&failed_destination), b"partial").unwrap();
    f.repository
        .create(&failed_session("failed", &failed_destination, None))
        .await
        .unwrap();
    let done_destination = f.dir.path().join("done.gguf");
    std::fs::write(&done_destination, b"finished").unwrap();
    let mut done = DownloadSession::new(
        "done".into(),
        URL.into(),
        done_destination.clone(),
        Some(8),
        None,
    )
    .unwrap();
    done.start().unwrap();
    done.complete().unwrap();
    f.repository.create(&done).await.unwrap();

    f.manager.delete_download("failed").await.unwrap();
    f.manager.delete_download("done").await.unwrap();

    assert!(!part_path(&failed_destination).exists());
    assert!(done_destination.exists());
}

#[tokio::test]
async fn superseding_a_session_keeps_its_part_for_the_new_one_to_resume() {
    let f = fixture();
    f.engine.set_stall(true);
    let destination = f.dir.path().join("model.gguf");
    std::fs::write(part_path(&destination), vec![1u8; 400]).unwrap();
    f.repository
        .create(&failed_session("old", &destination, Some(checksum('a'))))
        .await
        .unwrap();

    f.manager
        .start_download(request(&destination, Some(checksum('a'))))
        .await
        .unwrap();
    let call = first_call(&f).await;

    assert!(f.repository.get("old").await.unwrap().is_none());
    assert_eq!(call.resume_from, Some(400));
}

/// Sha-256 of 1000 bytes of 0x01, which `std::fs::write(.., vec![1u8; 1000])`
/// leaves on disk.
fn digest_of_ones() -> Checksum {
    use sha2::{Digest, Sha256};
    let digest = format!("{:x}", Sha256::digest(vec![1u8; 1000]));
    Checksum::new(ChecksumAlgorithm::Sha256, digest).unwrap()
}

#[tokio::test]
async fn a_complete_final_file_completes_with_no_transfer_and_no_rename() {
    for sum in [None, Some(digest_of_ones())] {
        let f = fixture();
        let destination = f.dir.path().join("model.gguf");
        std::fs::write(&destination, vec![1u8; 1000]).unwrap();
        let before = std::fs::metadata(&destination).unwrap().modified().unwrap();

        let id = f
            .manager
            .start_download(request(&destination, sum.clone()))
            .await
            .unwrap();
        let session = settled(&f, &id).await;

        assert_eq!(session.state(), &DownloadState::Completed);
        assert!(f.engine.get_download_calls().is_empty(), "no transfer");
        assert!(!part_path(&destination).exists());
        assert_eq!(
            std::fs::metadata(&destination).unwrap().modified().unwrap(),
            before
        );
    }
}

#[tokio::test]
async fn a_failed_rename_fails_the_session_and_keeps_the_part() {
    let f = fixture();
    let destination = f.dir.path().join("model.gguf");
    // A non-empty directory in the way makes the rename (and the fallback) fail.
    std::fs::create_dir_all(destination.join("inside")).unwrap();
    std::fs::write(part_path(&destination), vec![1u8; 1000]).unwrap();
    let mut pinned = request(&destination, Some(checksum('a')));
    pinned.expected_bytes = Some(1000);

    let id = f.manager.start_download(pinned).await.unwrap();
    let session = settled(&f, &id).await;

    assert_eq!(session.state(), &DownloadState::Failed);
    assert!(part_path(&destination).exists(), "verified bytes are kept");
    assert_eq!(
        session.error_message().unwrap(),
        "The download finished but the file couldn't be moved into place. Try again."
    );
}

#[tokio::test]
async fn a_session_with_no_checksum_does_not_reuse_leftover_bytes() {
    let f = fixture();
    f.engine.set_stall(true);

    // A leftover `.part` is deleted when the new session registers.
    let with_part = f.dir.path().join("a.gguf");
    std::fs::write(part_path(&with_part), vec![1u8; 400]).unwrap();
    f.manager
        .start_download(request(&with_part, None))
        .await
        .unwrap();
    assert!(!part_path(&with_part).exists());

    // An in-place partial is not migrated.
    let in_place = f.dir.path().join("b.gguf");
    std::fs::write(&in_place, vec![1u8; 500]).unwrap();
    let mut second = request(&in_place, None);
    second.url = "https://example.com/b.gguf".into();
    f.engine
        .set_file_size("https://example.com/b.gguf", Some(1000));
    f.manager.start_download(second).await.unwrap();
    let call = loop {
        if let Some(call) = f
            .engine
            .get_download_calls()
            .into_iter()
            .find(|call| call.url.ends_with("b.gguf"))
        {
            break call;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(call.resume_from, None);
    assert!(in_place.exists());
}

#[tokio::test]
async fn removing_a_failed_row_keeps_the_part_while_a_live_session_shares_its_destination() {
    let f = fixture();
    f.engine.set_stall(true);
    let destination = f.dir.path().join("model.gguf");
    f.manager
        .start_download(request(&destination, Some(checksum('a'))))
        .await
        .unwrap();
    std::fs::write(part_path(&destination), vec![1u8; 300]).unwrap();
    f.repository
        .create(&failed_session("old-row", &destination, None))
        .await
        .unwrap();

    f.manager.delete_download("old-row").await.unwrap();

    assert!(part_path(&destination).exists());
}

#[tokio::test]
async fn a_network_failure_reads_as_one_plain_sentence() {
    let f = fixture();
    f.engine
        .set_network_error("HEAD request failed: dns error: no such host".into());
    let destination = f.dir.path().join("model.gguf");

    let id = f
        .manager
        .start_download(request(&destination, None))
        .await
        .unwrap();
    let session = settled(&f, &id).await;

    assert_eq!(session.state(), &DownloadState::Failed);
    assert_eq!(
        session.error_message().unwrap(),
        "Couldn't reach the download host. Check your connection and try again."
    );
}

#[tokio::test]
async fn the_transfer_is_given_the_known_total() {
    let f = fixture();
    f.engine.set_stall(true);
    let destination = f.dir.path().join("model.gguf");
    let mut pinned = request(&destination, None);
    pinned.expected_bytes = Some(777);

    f.manager.start_download(pinned).await.unwrap();
    let call = first_call(&f).await;

    assert_eq!(call.known_total, Some(777));
}

#[tokio::test]
async fn removing_a_failed_row_discards_and_deletes_under_the_queue_gate() {
    let f = fixture();
    let manager = Arc::new(f.manager);
    let destination = f.dir.path().join("model.gguf");
    std::fs::write(part_path(&destination), b"partial").unwrap();
    f.repository
        .create(&failed_session("failed", &destination, None))
        .await
        .unwrap();

    let gate = manager.queue_gate.clone();
    let held = gate.lock().await;
    let remover = {
        let manager = manager.clone();
        tokio::spawn(async move { manager.delete_download("failed").await })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;

    // A start of the same file would be waiting on the gate too: neither the
    // discard nor the delete has happened yet.
    assert!(part_path(&destination).exists());
    assert!(f.repository.get("failed").await.unwrap().is_some());

    drop(held);
    remover.await.unwrap().unwrap();
    assert!(!part_path(&destination).exists());
    assert!(f.repository.get("failed").await.unwrap().is_none());
}
