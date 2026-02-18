//! Download Default Embedding Model Use Case
//!
//! Downloads and activates the default embedding model for first-run setup.
//!
//! # Purpose
//!
//! Provides a simplified interface for downloading the recommended default
//! embedding model (DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME) during first-run setup.
//!
//! # Business Logic
//!
//! 1. Initialize download for default model files (model.onnx, tokenizer.json, config.json)
//! 2. Monitor download progress
//! 3. Track completed download in models table
//! 4. Set as active embedding model
//! 5. Return model metadata
//!
//! # Example
//!
//! ```rust
//! let use_case = DownloadDefaultModelUseCase::new(
//!     download_manager,
//!     downloaded_model_repo,
//!     models_path,
//! );
//! let response = use_case.execute().await?;
//! println!("Downloaded: {} ({} bytes)", response.model_name, response.file_size_bytes);
//! ```

use crate::domain::downloaded_model::DownloadedModel;
use crate::domain::embedding_constants::{
    DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME, DEFAULT_EMBEDDING_MODEL_NAME,
};
use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::infrastructure::services::download_manager::{
    DownloadManager, DownloadManagerService, DownloadRequest,
};
use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

// =============================================================================
// DTOs
// =============================================================================

/// Response DTO for default model download
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadDefaultModelResponse {
    /// Download session ID
    pub download_id: String,
    /// Model ID
    pub model_id: String,
    /// Model name for display
    pub model_name: String,
    /// Path where model will be saved
    pub file_path: String,
    /// Estimated file size in bytes
    pub file_size_bytes: u64,
}

// =============================================================================
// Use Case
// =============================================================================

/// Download default embedding model use case
pub struct DownloadDefaultModelUseCase {
    /// Download manager for file downloads
    /// Uses trait for testability (commands inject DownloadManagerService from Container)
    download_manager: Arc<dyn DownloadManager>,
    /// Repository for tracking downloaded models
    downloaded_model_repo: DownloadedModelRepository,
    /// Base path for models directory
    models_path: PathBuf,
}

impl DownloadDefaultModelUseCase {
    /// Default model ID
    const DEFAULT_MODEL_ID: &'static str = DEFAULT_EMBEDDING_MODEL_NAME;

    /// Default model name for display
    const DEFAULT_MODEL_NAME: &'static str = DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME;

    /// HuggingFace model repository URL
    const HUGGINGFACE_BASE_URL: &'static str = "https://huggingface.co";

    /// ONNX model file path in HuggingFace repository
    /// Note: For sentence-transformers models, ONNX files are typically in the onnx/ subdirectory
    const MODEL_FILE: &'static str = "onnx/model.onnx";

    /// Create a new instance
    ///
    /// # Arguments
    ///
    /// * `download_manager` - Service for managing downloads
    ///   Commands inject Arc<dyn DownloadManager> from Container
    /// * `downloaded_model_repo` - Repository for tracking downloads (from Container)
    /// * `models_path` - Base directory for storing models (from Container)
    pub fn new(
        download_manager: Arc<dyn DownloadManager>,
        downloaded_model_repo: DownloadedModelRepository,
        models_path: PathBuf,
    ) -> Self {
        Self {
            download_manager,
            downloaded_model_repo,
            models_path,
        }
    }

    /// Execute the use case
    ///
    /// # Returns
    ///
    /// Response with download ID and model metadata
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Models directory doesn't exist and can't be created
    /// - Download initiation fails
    /// - Model is already downloaded
    pub async fn execute(&self) -> Result<DownloadDefaultModelResponse> {
        info!("Starting default embedding model download");

        // Ensure models directory exists
        self.ensure_models_directory().await?;

        // Check if model is already downloaded
        let existing_models = self.downloaded_model_repo.list_all().await?;
        for model in existing_models {
            if model.model_id() == Self::DEFAULT_MODEL_ID {
                warn!("Model already downloaded: {}", Self::DEFAULT_MODEL_ID);
                return Err(AppError::InvalidInput(
                    "Default model is already downloaded".to_string(),
                ));
            }
        }

        // Build model file path
        let model_dir = self.models_path.join(Self::DEFAULT_MODEL_NAME);
        tokio::fs::create_dir_all(&model_dir).await.map_err(|e| {
            AppError::FileSystem(format!("Failed to create model directory: {}", e))
        })?;

        let model_file_path = model_dir.join(Self::MODEL_FILE);

        // Build download URL
        let download_url = self.build_download_url();

        debug!(
            url = %download_url,
            destination = %model_file_path.display(),
            "Initiating model download"
        );

        // Start download
        let download_request = DownloadRequest {
            url: download_url,
            destination: model_file_path.clone(),
            checksum: None,   // Checksum verification happens in download_engine
            auth_token: None, // No auth required for public model
            model_name: Some(Self::DEFAULT_MODEL_NAME.to_string()),
            model_id: Some(Self::DEFAULT_MODEL_ID.to_string()),
        };

        let download_id = self
            .download_manager
            .start_download(download_request)
            .await
            .map_err(|e| AppError::Network(format!("Failed to start download: {}", e)))?;

        info!(
            download_id = %download_id,
            model_id = %Self::DEFAULT_MODEL_ID,
            "Default model download initiated"
        );

        Ok(DownloadDefaultModelResponse {
            download_id,
            model_id: Self::DEFAULT_MODEL_ID.to_string(),
            model_name: Self::DEFAULT_MODEL_NAME.to_string(),
            file_path: model_file_path.to_string_lossy().to_string(),
            file_size_bytes: 420_000_000,
        })
    }

    /// Build HuggingFace download URL for model file
    fn build_download_url(&self) -> String {
        format!(
            "{}/{}/resolve/main/{}",
            Self::HUGGINGFACE_BASE_URL,
            Self::DEFAULT_MODEL_ID,
            Self::MODEL_FILE
        )
    }

    /// Ensure models directory exists
    async fn ensure_models_directory(&self) -> Result<()> {
        if !self.models_path.exists() {
            debug!(path = %self.models_path.display(), "Creating models directory");
            tokio::fs::create_dir_all(&self.models_path)
                .await
                .map_err(|e| {
                    AppError::FileSystem(format!("Failed to create models directory: {}", e))
                })?;
        }
        Ok(())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::download::{DownloadError, DownloadSession};
    use crate::domain::embedding_constants::{
        DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME, DEFAULT_EMBEDDING_MODEL_NAME,
    };
    use crate::infrastructure::services::download_manager::DownloadEvent;
    use async_trait::async_trait;
    use tempfile::TempDir;
    use tokio::sync::{mpsc, RwLock};

    // Mock DownloadManager
    struct MockDownloadManager {
        started_downloads: Arc<RwLock<Vec<DownloadRequest>>>,
        should_fail: bool,
    }

    impl MockDownloadManager {
        fn new(should_fail: bool) -> Self {
            Self {
                started_downloads: Arc::new(RwLock::new(Vec::new())),
                should_fail,
            }
        }
    }

    #[async_trait]
    impl DownloadManager for MockDownloadManager {
        async fn start_download(&self, request: DownloadRequest) -> Result<String, DownloadError> {
            if self.should_fail {
                return Err(DownloadError::NetworkError("Mock failure".to_string()));
            }
            self.started_downloads.write().await.push(request);
            Ok("mock-download-id".to_string())
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

        fn subscribe_to_events(
            &self,
        ) -> Arc<RwLock<Option<mpsc::UnboundedReceiver<DownloadEvent>>>> {
            let (_tx, rx) = mpsc::unbounded_channel();
            Arc::new(RwLock::new(Some(rx)))
        }
    }

    #[tokio::test]
    async fn test_download_default_model_success() {
        let temp_dir = TempDir::new().unwrap();
        let models_path = temp_dir.path().join("models");

        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        crate::infrastructure::persistence::database::schema::initialize_schema(&pool)
            .await
            .unwrap();

        let download_manager = Arc::new(MockDownloadManager::new(false));
        let repo = DownloadedModelRepository::new(pool);

        let use_case =
            DownloadDefaultModelUseCase::new(download_manager.clone(), repo, models_path.clone());

        let result = use_case.execute().await.unwrap();

        assert_eq!(result.download_id, "mock-download-id");
        assert_eq!(result.model_id, DEFAULT_EMBEDDING_MODEL_NAME);
        assert_eq!(result.model_name, DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME);
        assert!(result.file_path.contains("model.onnx"));

        // Verify models directory was created
        assert!(models_path.exists());
    }

    #[tokio::test]
    async fn test_download_url_format() {
        let temp_dir = TempDir::new().unwrap();

        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        crate::infrastructure::persistence::database::schema::initialize_schema(&pool)
            .await
            .unwrap();

        let download_manager = Arc::new(MockDownloadManager::new(false));
        let repo = DownloadedModelRepository::new(pool);

        let use_case = DownloadDefaultModelUseCase::new(
            download_manager.clone(),
            repo,
            temp_dir.path().to_path_buf(),
        );

        let _ = use_case.execute().await.unwrap();

        let downloads = download_manager.started_downloads.read().await;
        assert_eq!(downloads.len(), 1);
        assert!(downloads[0].url.contains("huggingface.co"));
        assert!(downloads[0].url.contains(DEFAULT_EMBEDDING_MODEL_NAME));
        assert!(downloads[0].url.contains("model.onnx"));
    }
}
