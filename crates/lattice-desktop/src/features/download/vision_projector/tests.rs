//! Starting a projector download: what is fetched, where it lands, and when
//! nothing is started.

use super::*;
use crate::infrastructure::persistence::repositories::downloaded_models::DownloadedModelRepository as SqliteModels;
use async_trait::async_trait;
use lattice_core::application::ports::download_manager::{DownloadBatchItem, DownloadEvent};
use lattice_core::domain::download::{DownloadError, DownloadSession};
use lattice_core::domain::models::downloaded::ModelLocation;
use lattice_core::domain::DownloadedModel;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, RwLock};

/// Records every download it is asked to start.
#[derive(Default)]
struct Recorder {
    started: Mutex<Vec<DownloadRequest>>,
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
        self.started.lock().await.push(request);
        Ok("projector-download".into())
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
        Ok(Vec::new())
    }
    async fn list_active_downloads(
        &self,
    ) -> std::result::Result<Vec<DownloadSession>, DownloadError> {
        Ok(Vec::new())
    }
    async fn retry_download(&self, _id: &str) -> std::result::Result<(), DownloadError> {
        Ok(())
    }
    async fn delete_download(&self, _id: &str) -> std::result::Result<(), DownloadError> {
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

/// A migrated in-memory repository holding one local model at `path`.
async fn repository_with(path: PathBuf) -> SqliteModels {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let repository = SqliteModels::new(pool);
    let row = DownloadedModel::new(
        "row-1".into(),
        "Qwen 3.5 9B".into(),
        "qwen3.5-9b-q4_k_m".into(),
        ModelLocation::LocalFile { path },
        4,
        "GGUF".into(),
        None,
    )
    .unwrap();
    repository.save(&row).await.unwrap();
    repository
}

/// A catalog download of the 9B under a temporary models directory, with a
/// sparse projector of `projector_len` bytes beside it when given.
fn catalog_9b(projector_len: Option<u64>) -> (tempfile::TempDir, PathBuf) {
    let models = tempfile::tempdir().unwrap();
    let directory = models.path().join("qwen3.5-9b-q4_k_m");
    std::fs::create_dir_all(&directory).unwrap();
    let weights = directory.join("Qwen3.5-9B-Q4_K_M.gguf");
    std::fs::write(&weights, b"GGUF").unwrap();
    if let Some(len) = projector_len {
        std::fs::File::create(directory.join("mmproj.gguf"))
            .unwrap()
            .set_len(len)
            .unwrap();
    }
    (models, weights)
}

#[tokio::test]
async fn a_missing_projector_is_fetched_beside_the_weights_with_its_digest() {
    let (_models, weights) = catalog_9b(Some(1_000));
    let repository = repository_with(weights.clone()).await;
    let manager = Recorder::default();
    let started = start_vision_projector_download_impl(&manager, &repository, "row-1")
        .await
        .unwrap();
    assert_eq!(started.download_id.as_deref(), Some("projector-download"));
    let requests = manager.started.lock().await;
    let request = requests.first().expect("one download started");
    assert_eq!(request.destination, weights.with_file_name("mmproj.gguf"));
    assert_eq!(
        request.url,
        "https://huggingface.co/unsloth/Qwen3.5-9B-GGUF/resolve/main/mmproj-F16.gguf"
    );
    assert!(request.checksum.is_some());
    assert!(request.model_id.is_none(), "no model record waits on it");
}

#[tokio::test]
async fn a_projector_already_on_disk_starts_nothing() {
    let (_models, weights) = catalog_9b(Some(918_166_080));
    let repository = repository_with(weights).await;
    let manager = Recorder::default();
    let started = start_vision_projector_download_impl(&manager, &repository, "row-1")
        .await
        .unwrap();
    assert_eq!(started.download_id, None);
    assert!(manager.started.lock().await.is_empty());
}

#[tokio::test]
async fn a_model_the_catalog_gives_no_projector_is_refused() {
    let models = tempfile::tempdir().unwrap();
    let weights = models.path().join("custom").join("my-model.gguf");
    std::fs::create_dir_all(weights.parent().unwrap()).unwrap();
    std::fs::write(&weights, b"GGUF").unwrap();
    let repository = repository_with(weights).await;
    let manager = Recorder::default();
    let refused = start_vision_projector_download_impl(&manager, &repository, "row-1")
        .await
        .unwrap_err();
    assert!(matches!(refused, AppError::InvalidInput(_)), "{refused}");
    assert!(manager.started.lock().await.is_empty());
    let missing = start_vision_projector_download_impl(&manager, &repository, "row-2")
        .await
        .unwrap_err();
    assert!(matches!(missing, AppError::NotFound(_)), "{missing}");
}
