//! A model's rows land together: its file rows with it, its failure with its files.

use super::*;
use crate::features::llm::engine::HardcodedModelCatalog;
use crate::infrastructure::adapters::fs::{TokioChecksumAdapter, TokioFileSystemAdapter};
use crate::infrastructure::file_system::FileSystemAdapter;
use crate::infrastructure::persistence::repositories::downloaded_models::DownloadedModelRepository;
use async_trait::async_trait;
use lattice_core::application::ports::download_manager::DownloadEvent;
use lattice_core::domain::download::{DownloadError, DownloadSession};
use lattice_core::domain::models::metadata::ModelFileMetadata;
use lattice_core::domain::models::selection::{
    ModelCategory, ModelFormat, ModelMetadata, PerformanceTier,
};
use tokio::sync::{mpsc, RwLock};

/// A download manager that refuses every start.
struct RefusingDownloads;

#[async_trait]
impl DownloadManager for RefusingDownloads {
    async fn start_download(&self, _request: DownloadRequest) -> Result<String, DownloadError> {
        Err(DownloadError::InvalidUrl("refused".into()))
    }
    async fn enqueue_download(&self, _request: DownloadRequest) -> Result<String, DownloadError> {
        Err(DownloadError::InvalidUrl("refused".into()))
    }
    async fn start_download_batch(
        &self,
        _items: Vec<DownloadBatchItem>,
    ) -> Result<Vec<String>, DownloadError> {
        Err(DownloadError::InvalidUrl("refused".into()))
    }
    async fn pause_download(&self, _id: &str) -> Result<(), DownloadError> {
        Ok(())
    }
    async fn resume_download(&self, _id: &str) -> Result<(), DownloadError> {
        Ok(())
    }
    async fn cancel_download(&self, _id: &str) -> Result<(), DownloadError> {
        Ok(())
    }
    async fn get_download_status(
        &self,
        _id: &str,
    ) -> Result<Option<DownloadSession>, DownloadError> {
        Ok(None)
    }
    async fn list_downloads(&self) -> Result<Vec<DownloadSession>, DownloadError> {
        Ok(Vec::new())
    }
    async fn list_active_downloads(&self) -> Result<Vec<DownloadSession>, DownloadError> {
        Ok(Vec::new())
    }
    async fn retry_download(&self, _id: &str) -> Result<(), DownloadError> {
        Ok(())
    }
    async fn delete_download(&self, _id: &str) -> Result<(), DownloadError> {
        Ok(())
    }
    async fn clear_completed_downloads(&self) -> Result<usize, DownloadError> {
        Ok(0)
    }
    async fn delete_pending_by_model(&self, _model_id: &str) -> Result<u64, DownloadError> {
        Ok(0)
    }
    async fn process_pending_queue(&self) -> Result<(), DownloadError> {
        Ok(())
    }
    fn subscribe_to_events(&self) -> Arc<RwLock<Option<mpsc::Receiver<DownloadEvent>>>> {
        let (_tx, rx) = mpsc::channel(1);
        Arc::new(RwLock::new(Some(rx)))
    }
}

/// A keychain with nothing in it.
struct EmptyKeychain;

#[async_trait]
impl CredentialsPort for EmptyKeychain {
    async fn set_api_key(&self, _service: &str, _key: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn get_api_key(&self, _service: &str) -> Result<Option<String>, AppError> {
        Ok(None)
    }
    async fn delete_api_key(&self, _service: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn has_api_key(&self, _service: &str) -> Result<bool, AppError> {
        Ok(false)
    }
    async fn clear_all_credentials(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn set_custom_endpoint(&self, _endpoint: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn get_custom_endpoint(&self) -> Result<Option<String>, AppError> {
        Ok(None)
    }
}

async fn use_case() -> (DownloadModelUseCase, SqlitePool) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let use_case = DownloadModelUseCase::new(
        Arc::new(HardcodedModelCatalog::new()),
        Arc::new(DownloadedModelRepository::new(pool.clone())),
        Arc::new(RefusingDownloads),
        Arc::new(FileSystemAdapter::new()),
        Arc::new(EmptyKeychain),
        Arc::new(TokioChecksumAdapter),
        Arc::new(TokioFileSystemAdapter),
        pool.clone(),
    );
    (use_case, pool)
}

fn curated(files: Vec<ModelFileMetadata>) -> ModelMetadata {
    ModelMetadata {
        id: "write-test".into(),
        name: "Write Test".into(),
        category: ModelCategory::Embedding,
        description: "a synthetic model".into(),
        size_gb: 0.0,
        context_length: 512,
        performance_tier: PerformanceTier::Balanced,
        supported_quantizations: vec![],
        capabilities: vec![],
        download_url: Some("https://example.com/write-test".into()),
        license: "unknown".into(),
        requires_auth: false,
        model_id: None,
        default_filename: Some("weights.gguf".into()),
        files,
        total_size_bytes: 10,
        embedding_dimensions: None,
        embedding_compatibility: None,
        format: ModelFormat::Gguf,
    }
}

fn two_files() -> Vec<ModelFileMetadata> {
    vec![
        ModelFileMetadata::new("weights.gguf".into(), "https://example.com/w".into(), 8),
        ModelFileMetadata::new("tokenizer.json".into(), "https://example.com/t".into(), 2),
    ]
}

/// Rows of this model only: the init migration seeds a bundled model of its own.
async fn models_with_id(pool: &SqlitePool, model_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM models WHERE model_id = ?")
        .bind(model_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn refuse_file_rows(pool: &SqlitePool) {
    sqlx::raw_sql(
        "CREATE TRIGGER refuse_file_rows BEFORE INSERT ON model_files
         BEGIN SELECT RAISE(ABORT, 'injected'); END;",
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn refuse_failed_files(pool: &SqlitePool) {
    sqlx::raw_sql(
        "CREATE TRIGGER refuse_failed_files BEFORE UPDATE OF status ON model_files
         WHEN NEW.status = 'failed'
         BEGIN SELECT RAISE(ABORT, 'injected'); END;",
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn model_status(pool: &SqlitePool, model_id: &str) -> String {
    sqlx::query_scalar("SELECT status FROM models WHERE model_id = ?")
        .bind(model_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_single_file_model_row_does_not_land_without_its_file_row() {
    let (use_case, pool) = use_case().await;
    refuse_file_rows(&pool).await;
    let id = "write-test-single-pending";
    let paths = ModelPaths::new(id).unwrap();

    let result = use_case
        .download_single_file_model(id, &curated(vec![]), paths.unified_path(), &paths)
        .await;

    assert!(result.is_err());
    assert_eq!(models_with_id(&pool, id).await, 0);
}

#[tokio::test]
async fn a_single_file_model_that_cannot_start_is_failed_with_its_file_or_not_at_all() {
    let (use_case, pool) = use_case().await;
    let id = "write-test-single-failed";
    let paths = ModelPaths::new(id).unwrap();
    refuse_failed_files(&pool).await;

    let result = use_case
        .download_single_file_model(id, &curated(vec![]), paths.unified_path(), &paths)
        .await;

    assert!(result.is_err());
    assert_eq!(model_status(&pool, id).await, "pending");
}

#[tokio::test]
async fn a_multi_file_model_row_does_not_land_without_its_file_rows() {
    let (use_case, pool) = use_case().await;
    refuse_file_rows(&pool).await;
    let id = "write-test-multi-pending";
    let paths = ModelPaths::new(id).unwrap();

    let result = use_case
        .download_multi_file_model(id, &curated(two_files()), paths.unified_path(), &paths)
        .await;

    assert!(result.is_err());
    assert_eq!(models_with_id(&pool, id).await, 0);
}

#[tokio::test]
async fn a_multi_file_model_that_cannot_start_is_failed_with_its_files_or_not_at_all() {
    let (use_case, pool) = use_case().await;
    let id = "write-test-multi-failed";
    let paths = ModelPaths::new(id).unwrap();
    refuse_failed_files(&pool).await;

    let result = use_case
        .download_multi_file_model(id, &curated(two_files()), paths.unified_path(), &paths)
        .await;

    assert!(result.is_err());
    assert_eq!(model_status(&pool, id).await, "pending");
}
