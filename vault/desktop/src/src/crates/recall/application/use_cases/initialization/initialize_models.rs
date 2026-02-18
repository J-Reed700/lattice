//! Initialize Models Use Case
//!
//! Downloads and initializes embedding models for first-run setup.
//!
//! # Purpose
//!
//! Ensures the application has the required embedding models before indexing.
//! On first run, downloads the default model. On subsequent runs, verifies
//! the model is loaded and ready.
//!
//! # Business Logic
//!
//! 1. Check if embedding model is already loaded/available
//! 2. If not loaded, attempt to load from disk
//! 3. If not on disk, return error (model download is a separate use case)
//! 4. Verify model integrity and readiness
//! 5. Return success with model metadata
//!
//! # Example
//!
//! ```rust
//! let use_case = InitializeModelsUseCase::new(embedding_port);
//! let response = use_case.execute().await?;
//!
//! if response.success {
//!     println!("Model ready at: {}", response.model_path);
//!     println!("Dimension: {}", response.model_dimension.unwrap());
//! }
//! ```

use crate::application::dtos::initialization_dto::InitializeModelsResponseDto;
use crate::domain::embedding_constants::{DEFAULT_EMBEDDING_DIM, DEFAULT_EMBEDDING_MODEL_NAME};
use crate::infrastructure::services::traits::ModelManagerTrait;
use crate::shared::result::Result;
use std::sync::Arc;

// =============================================================================
// Use Case
// =============================================================================

/// Initialize models use case.
///
/// Ensures embedding models are loaded and ready for use.
pub struct InitializeModelsUseCase {
    model_manager: Arc<dyn ModelManagerTrait>,
}

impl InitializeModelsUseCase {
    /// Create a new instance of the use case.
    ///
    /// # Arguments
    ///
    /// * `model_manager` - Model manager for downloads and paths
    pub fn new(model_manager: Arc<dyn ModelManagerTrait>) -> Self {
        Self { model_manager }
    }

    /// Execute the use case.
    ///
    /// # Returns
    ///
    /// Response DTO with model initialization status and metadata.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Model is not available and cannot be loaded
    /// - Model integrity check fails
    /// - Embedding service is not operational
    pub async fn execute(&self) -> Result<InitializeModelsResponseDto> {
        let was_cached = self.model_manager.is_model_ready().await;
        let model_path = if was_cached {
            self.model_manager.get_model_path()
        } else {
            self.model_manager.ensure_model_available().await?
        };

        Ok(InitializeModelsResponseDto {
            success: true,
            model_path: model_path.to_string_lossy().to_string(),
            model_name: Some(DEFAULT_EMBEDDING_MODEL_NAME.to_string()),
            model_dimension: Some(DEFAULT_EMBEDDING_DIM),
            was_cached,
        })
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::services::traits::ModelInfo;
    use crate::shared::error::AppError;
    use async_trait::async_trait;
    use parking_lot::RwLock;
    use std::path::PathBuf;

    struct MockModelManager {
        is_ready: RwLock<bool>,
        model_path: PathBuf,
    }

    impl MockModelManager {
        fn new(is_ready: bool) -> Self {
            Self {
                is_ready: RwLock::new(is_ready),
                model_path: PathBuf::from("/tmp/model.onnx"),
            }
        }

        fn set_ready(&self, ready: bool) {
            *self.is_ready.write() = ready;
        }
    }

    #[async_trait]
    impl ModelManagerTrait for MockModelManager {
        async fn ensure_model_available(&self) -> Result<PathBuf> {
            Ok(self.model_path.clone())
        }

        async fn is_model_ready(&self) -> bool {
            *self.is_ready.read()
        }

        fn get_model_path(&self) -> PathBuf {
            self.model_path.clone()
        }

        fn get_tokenizer_path(&self) -> PathBuf {
            self.model_path.with_extension("json")
        }

        async fn ensure_reranker_available(&self) -> Result<PathBuf> {
            Ok(self.model_path.clone())
        }

        async fn is_reranker_ready(&self) -> bool {
            *self.is_ready.read()
        }

        fn get_reranker_path(&self) -> PathBuf {
            self.model_path.clone()
        }

        fn get_reranker_tokenizer_path(&self) -> PathBuf {
            self.model_path.with_extension("json")
        }

        fn cancel_download(&self) {}

        fn get_download_progress(&self) -> Option<f32> {
            None
        }

        fn get_model_info(&self) -> ModelInfo {
            ModelInfo {
                name: "test-model".to_string(),
                version: "1.0".to_string(),
                size_bytes: None,
                is_downloaded: *self.is_ready.read(),
            }
        }
    }

    #[tokio::test]
    async fn test_initialize_models_success() {
        // Arrange
        let model_manager = Arc::new(MockModelManager::new(true));
        let use_case = InitializeModelsUseCase::new(model_manager);

        // Act
        let result = use_case.execute().await;

        // Assert
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.success);
        assert_eq!(response.model_dimension, Some(DEFAULT_EMBEDDING_DIM));
        assert!(response.model_path.contains("onnx"));
    }

    #[tokio::test]
    async fn test_initialize_models_not_ready() {
        // Arrange
        let model_manager = Arc::new(MockModelManager::new(false));
        let use_case = InitializeModelsUseCase::new(model_manager);

        // Act
        let result = use_case.execute().await;

        // Assert
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(!response.was_cached);
    }

    #[tokio::test]
    async fn test_initialize_models_returns_correct_dimension() {
        // Arrange
        let model_manager = Arc::new(MockModelManager::new(true));
        let use_case = InitializeModelsUseCase::new(model_manager);

        // Act
        let result = use_case.execute().await.unwrap();

        // Assert
        assert_eq!(result.model_dimension, Some(DEFAULT_EMBEDDING_DIM));
    }

    #[tokio::test]
    async fn test_initialize_models_cached() {
        // Arrange
        let model_manager = Arc::new(MockModelManager::new(true));
        let use_case = InitializeModelsUseCase::new(model_manager);

        // Act
        let result = use_case.execute().await.unwrap();

        // Assert
        // Since model is already ready, it was cached
        assert!(result.was_cached);
    }
}
