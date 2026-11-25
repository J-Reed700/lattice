//! The reranker's status and download start, against a temp models directory
//! and a recording download manager.

use super::*;
use async_trait::async_trait;
use lattice_core::application::ports::download_manager::{DownloadBatchItem, DownloadEvent};
use lattice_core::domain::download::{DownloadError, DownloadSession, DownloadState};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, RwLock};

/// Records every start, and fails them when `fail` is set.
#[derive(Default)]
struct Recorder {
    started: Mutex<Vec<DownloadRequest>>,
    fail: bool,
    /// What `list_downloads` returns.
    sessions: Mutex<Vec<DownloadSession>>,
    /// Every id `delete_download` was given.
    deleted: Mutex<Vec<String>>,
}

#[async_trait]
impl DownloadManager for Recorder {
    async fn start_download(
        &self,
        request: DownloadRequest,
    ) -> std::result::Result<String, DownloadError> {
        self.enqueue_download(request).await
    }
    async fn start_download_batch(
        &self,
        _items: Vec<DownloadBatchItem>,
    ) -> std::result::Result<Vec<String>, DownloadError> {
        Ok(Vec::new())
    }
    async fn enqueue_download(
        &self,
        request: DownloadRequest,
    ) -> std::result::Result<String, DownloadError> {
        if self.fail {
            return Err(DownloadError::NetworkError("refused".into()));
        }
        self.started.lock().unwrap().push(request);
        Ok("reranker-download".into())
    }
    async fn pause_download(&self, _id: &str) -> std::result::Result<(), DownloadError> {
        Ok(())
    }
    async fn resume_download(&self, _id: &str) -> std::result::Result<(), DownloadError> {
        Ok(())
    }
    async fn cancel_download(&self, _id: &str) -> std::result::Result<(), DownloadError> {
        Ok(())
    }
    async fn get_download_status(
        &self,
        _id: &str,
    ) -> std::result::Result<Option<DownloadSession>, DownloadError> {
        Ok(None)
    }
    async fn list_downloads(&self) -> std::result::Result<Vec<DownloadSession>, DownloadError> {
        Ok(self.sessions.lock().unwrap().clone())
    }
    async fn list_active_downloads(
        &self,
    ) -> std::result::Result<Vec<DownloadSession>, DownloadError> {
        Ok(Vec::new())
    }
    async fn retry_download(&self, _id: &str) -> std::result::Result<(), DownloadError> {
        Ok(())
    }
    async fn delete_download(&self, id: &str) -> std::result::Result<(), DownloadError> {
        self.deleted.lock().unwrap().push(id.to_owned());
        Ok(())
    }
    async fn clear_completed_downloads(&self) -> std::result::Result<usize, DownloadError> {
        Ok(0)
    }
    async fn delete_pending_by_model(
        &self,
        _model_id: &str,
    ) -> std::result::Result<u64, DownloadError> {
        Ok(0)
    }
    async fn process_pending_queue(&self) -> std::result::Result<(), DownloadError> {
        Ok(())
    }
    fn subscribe_to_events(&self) -> Arc<RwLock<Option<mpsc::Receiver<DownloadEvent>>>> {
        let (_tx, rx) = mpsc::channel(1);
        Arc::new(RwLock::new(Some(rx)))
    }
}

/// A models directory whose reranker file, when given, has `len` bytes.
fn models_with(len: Option<u64>) -> (tempfile::TempDir, PathBuf) {
    let models = tempfile::tempdir().unwrap();
    let gguf = reranker_gguf_path(models.path());
    if let Some(len) = len {
        std::fs::create_dir_all(gguf.parent().unwrap()).unwrap();
        std::fs::File::create(&gguf).unwrap().set_len(len).unwrap();
    }
    (models, gguf)
}

#[test]
fn the_status_names_the_model_and_what_it_costs() {
    let (models, gguf) = models_with(None);
    let status = reranker_status_impl(models.path());
    assert!(!status.installed);
    assert_eq!(status.model_name, "Qwen/Qwen3-Reranker-0.6B");
    assert_eq!(
        status.download_bytes, 639_153_184,
        "a size the UI can show before starting, not a placeholder"
    );
    assert_eq!(status.path, gguf.to_string_lossy());
}

#[test]
fn installed_means_the_exact_pinned_size() {
    for (len, installed) in [
        (None, false),
        (Some(CURATED_RERANKER.bytes - 1), false),
        (Some(CURATED_RERANKER.bytes + 1), false),
        (Some(CURATED_RERANKER.bytes), true),
    ] {
        let (models, gguf) = models_with(len);
        let status = reranker_status_impl(models.path());
        assert_eq!(status.installed, installed, "length {len:?}");
        assert_eq!(status.path, gguf.to_string_lossy());
    }
}

#[tokio::test]
async fn downloading_when_installed_starts_nothing() {
    let (models, _gguf) = models_with(Some(CURATED_RERANKER.bytes));
    let manager = Recorder::default();
    let status = download_reranker_impl(&manager, models.path())
        .await
        .unwrap();
    assert!(status.installed);
    assert!(manager.started.lock().unwrap().is_empty());
}

#[tokio::test]
async fn downloading_when_missing_starts_a_pinned_download() {
    let (models, gguf) = models_with(Some(10));
    let manager = Recorder::default();
    let status = download_reranker_impl(&manager, models.path())
        .await
        .unwrap();
    assert!(!status.installed);
    assert_eq!(status.path, gguf.to_string_lossy());
    let started = manager.started.lock().unwrap();
    let request = started.first().expect("one download started");
    assert_eq!(started.len(), 1);
    assert_eq!(request.url, CURATED_RERANKER.url());
    assert_eq!(request.destination, gguf);
    assert_eq!(
        request.checksum.as_ref().unwrap().value(),
        CURATED_RERANKER.sha256
    );
    assert_eq!(request.expected_bytes, Some(CURATED_RERANKER.bytes));
    assert_eq!(request.model_name.as_deref(), Some(CURATED_RERANKER.name));
    assert!(request.model_id.is_none());
}

#[tokio::test]
async fn the_old_downloaders_partial_file_is_removed() {
    let (models, gguf) = models_with(None);
    std::fs::create_dir_all(gguf.parent().unwrap()).unwrap();
    let leftover = gguf.with_extension("tmp");
    std::fs::write(&leftover, b"partial").unwrap();
    let manager = Recorder::default();
    download_reranker_impl(&manager, models.path())
        .await
        .unwrap();
    assert!(!leftover.exists());
}

#[tokio::test]
async fn a_start_error_propagates() {
    let (models, _gguf) = models_with(None);
    let manager = Recorder {
        fail: true,
        ..Recorder::default()
    };
    let error = download_reranker_impl(&manager, models.path())
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Couldn't reach the download host"),
        "{error}"
    );
}

/// A session for `destination`, driven into `state`.
fn session(id: &str, destination: &Path, state: DownloadState) -> DownloadSession {
    let mut session = DownloadSession::new(
        id.to_owned(),
        "https://example.com/model.gguf".to_owned(),
        destination.to_path_buf(),
        Some(10),
        None,
    )
    .unwrap();
    match state {
        DownloadState::Pending => {}
        DownloadState::Downloading => session.start().unwrap(),
        DownloadState::Paused => {
            session.start().unwrap();
            session.pause().unwrap();
        }
        DownloadState::Completed => {
            session.start().unwrap();
            session.complete().unwrap();
        }
        DownloadState::Failed => {
            session.start().unwrap();
            session.fail("boom".into()).unwrap();
        }
        DownloadState::Cancelled => session.cancel().unwrap(),
    }
    session
}

fn manager_with(sessions: Vec<DownloadSession>) -> Recorder {
    Recorder {
        sessions: Mutex::new(sessions),
        ..Recorder::default()
    }
}

#[tokio::test]
async fn removing_deletes_the_model_and_reports_it_gone() {
    let (models, gguf) = models_with(Some(CURATED_RERANKER.bytes));
    let status = remove_reranker_impl(&Recorder::default(), models.path())
        .await
        .unwrap();
    assert!(!gguf.exists());
    assert!(!status.installed);
    assert_eq!(status.path, gguf.to_string_lossy());
}

#[tokio::test]
async fn removing_is_refused_while_the_download_runs() {
    for state in [
        DownloadState::Pending,
        DownloadState::Downloading,
        DownloadState::Paused,
    ] {
        let (models, gguf) = models_with(Some(CURATED_RERANKER.bytes));
        let manager = manager_with(vec![
            session("done", &gguf, DownloadState::Completed),
            session("live", &gguf, state),
        ]);
        let error = remove_reranker_impl(&manager, models.path())
            .await
            .unwrap_err();
        assert!(
            matches!(&error, AppError::InvalidState(message) if message
                == "The reranker is still downloading. Cancel the download in Downloads, then remove it."),
            "{state:?}: {error:?}"
        );
        assert!(gguf.exists(), "{state:?}");
        assert!(manager.deleted.lock().unwrap().is_empty(), "{state:?}");
    }
}

#[tokio::test]
async fn removing_forgets_the_reranker_s_finished_downloads_only() {
    let (models, gguf) = models_with(Some(CURATED_RERANKER.bytes));
    let other = models.path().join("other.gguf");
    let manager = manager_with(vec![
        session("completed", &gguf, DownloadState::Completed),
        session("failed", &gguf, DownloadState::Failed),
        session("cancelled", &gguf, DownloadState::Cancelled),
        session("other", &other, DownloadState::Completed),
        session("other-live", &other, DownloadState::Downloading),
    ]);
    remove_reranker_impl(&manager, models.path()).await.unwrap();
    assert_eq!(
        *manager.deleted.lock().unwrap(),
        ["completed", "failed", "cancelled"]
    );
}

#[tokio::test]
async fn removing_clears_the_partial_files_too() {
    let (models, gguf) = models_with(Some(CURATED_RERANKER.bytes));
    let part = part_path(&gguf);
    let tmp = gguf.with_extension("tmp");
    std::fs::write(&part, b"partial").unwrap();
    std::fs::write(&tmp, b"partial").unwrap();
    remove_reranker_impl(&Recorder::default(), models.path())
        .await
        .unwrap();
    assert!(!part.exists() && !tmp.exists() && !gguf.exists());
}

#[tokio::test]
async fn removing_with_nothing_on_disk_succeeds() {
    let (models, _gguf) = models_with(None);
    let status = remove_reranker_impl(&Recorder::default(), models.path())
        .await
        .unwrap();
    assert!(!status.installed);
}

#[tokio::test]
async fn a_model_path_that_cannot_be_removed_is_an_error() {
    let (models, gguf) = models_with(None);
    std::fs::create_dir_all(&gguf).unwrap();
    let error = remove_reranker_impl(&Recorder::default(), models.path())
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::FileSystem(_)), "{error:?}");
    assert!(
        error.to_string().contains(&*gguf.to_string_lossy()),
        "{error}"
    );
}
