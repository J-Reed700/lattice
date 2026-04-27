//! # Model Storage Port
//!
//! Port for managing local model storage.
//!
//! ## Purpose
//!
//! Provides an abstraction for managing downloaded LLM models on the
//! local filesystem, including listing, validation, and deletion.
//!
//! ## Implementations
//!
//! - **Production**: Filesystem-based storage with integrity checks
//! - **Mock**: In-memory mock for testing
//!
//! ## Example
//!
//! ```rust,no_run
//! use crate::application::ports::ModelStoragePort;
//!
//! async fn check_model(storage: &dyn ModelStoragePort) -> Result<()> {
//!     if storage.is_model_downloaded("phi-3-mini").await? {
//!         let path = storage.get_model_path("phi-3-mini").await?;
//!         println!("Model found at: {:?}", path);
//!     }
//!     Ok(())
//! }
//! ```

use crate::shared::error::AppError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Information about a downloaded model.
#[derive(Debug, Clone)]
pub struct DownloadedModel {
    /// Model identifier
    pub model_id: String,
    /// Local filesystem path
    pub path: PathBuf,
    /// Model size in bytes
    pub size_bytes: u64,
    /// When the model was downloaded
    pub downloaded_at: DateTime<Utc>,
}

/// Port for managing local model storage.
///
/// Provides operations for listing, checking, and managing
/// downloaded models on the local filesystem.
#[async_trait]
pub trait ModelStoragePort: Send + Sync {
    /// List all downloaded models.
    ///
    /// # Returns
    /// - `Ok(Vec<DownloadedModel>)` - List of downloaded models
    /// - `Err(AppError)` - Failed to scan storage
    async fn list_models(&self) -> Result<Vec<DownloadedModel>, AppError>;

    /// Check if a model is downloaded.
    ///
    /// # Arguments
    /// - `model_id` - Model identifier to check
    ///
    /// # Returns
    /// - `Ok(true)` - Model is downloaded and valid
    /// - `Ok(false)` - Model is not downloaded
    /// - `Err(AppError)` - Failed to check storage
    async fn is_model_downloaded(&self, model_id: &str) -> Result<bool, AppError>;

    /// Get the filesystem path for a model.
    ///
    /// # Arguments
    /// - `model_id` - Model identifier
    ///
    /// # Returns
    /// - `Ok(PathBuf)` - Path to model directory
    /// - `Err(AppError::NotFound)` - Model not downloaded
    /// - `Err(AppError)` - Failed to access storage
    async fn get_model_path(&self, model_id: &str) -> Result<PathBuf, AppError>;

    /// Delete a downloaded model.
    ///
    /// # Arguments
    /// - `model_id` - Model identifier to delete
    ///
    /// # Returns
    /// - `Ok(())` - Model deleted successfully
    /// - `Err(AppError::NotFound)` - Model not found
    /// - `Err(AppError)` - Failed to delete model
    async fn delete_model(&self, model_id: &str) -> Result<(), AppError>;
}

/// Mock implementation for testing.
///
/// Provides an in-memory mock of model storage for testing
/// without filesystem operations.
pub struct MockModelStoragePort {
    models: Arc<Mutex<HashMap<String, DownloadedModel>>>,
}

impl MockModelStoragePort {
    /// Create a new empty mock storage.
    pub fn new() -> Self {
        Self {
            models: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create mock storage with pre-loaded models.
    pub fn with_models() -> Self {
        let mock = Self::new();

        // Add some test models
        mock.add_model(DownloadedModel {
            model_id: "phi-3-mini".to_string(),
            path: PathBuf::from("/models/phi-3-mini"),
            size_bytes: 1_800_000_000, // 1.8 GB
            downloaded_at: Utc::now(),
        });

        mock.add_model(DownloadedModel {
            model_id: "mistral-7b".to_string(),
            path: PathBuf::from("/models/mistral-7b"),
            size_bytes: 4_100_000_000, // 4.1 GB
            downloaded_at: Utc::now(),
        });

        mock
    }

    /// Add a model to mock storage (for testing).
    pub fn add_model(&self, model: DownloadedModel) {
        self.models.lock().insert(model.model_id.clone(), model);
    }

    /// Remove all models (for testing).
    pub fn clear(&self) {
        self.models.lock().clear();
    }

    /// Get the number of models in storage (for testing).
    pub fn model_count(&self) -> usize {
        self.models.lock().len()
    }
}

impl Default for MockModelStoragePort {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ModelStoragePort for MockModelStoragePort {
    async fn list_models(&self) -> Result<Vec<DownloadedModel>, AppError> {
        Ok(self.models.lock().values().cloned().collect())
    }

    async fn is_model_downloaded(&self, model_id: &str) -> Result<bool, AppError> {
        Ok(self.models.lock().contains_key(model_id))
    }

    async fn get_model_path(&self, model_id: &str) -> Result<PathBuf, AppError> {
        self.models
            .lock()
            .get(model_id)
            .map(|m| m.path.clone())
            .ok_or_else(|| AppError::NotFound(format!("Model not found: {}", model_id)))
    }

    async fn delete_model(&self, model_id: &str) -> Result<(), AppError> {
        self.models
            .lock()
            .remove(model_id)
            .map(|_| ())
            .ok_or_else(|| AppError::NotFound(format!("Model not found: {}", model_id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_storage_empty() {
        let storage = MockModelStoragePort::new();
        let models = storage.list_models().await.unwrap();

        assert_eq!(models.len(), 0);
    }

    #[tokio::test]
    async fn test_mock_storage_with_models() {
        let storage = MockModelStoragePort::with_models();
        let models = storage.list_models().await.unwrap();

        assert_eq!(models.len(), 2);
    }

    #[tokio::test]
    async fn test_mock_storage_is_downloaded() {
        let storage = MockModelStoragePort::with_models();

        assert!(storage.is_model_downloaded("phi-3-mini").await.unwrap());
        assert!(storage.is_model_downloaded("mistral-7b").await.unwrap());
        assert!(!storage.is_model_downloaded("nonexistent").await.unwrap());
    }

    #[tokio::test]
    async fn test_mock_storage_get_path() {
        let storage = MockModelStoragePort::with_models();
        let path = storage.get_model_path("phi-3-mini").await.unwrap();

        assert_eq!(path, PathBuf::from("/models/phi-3-mini"));
    }

    #[tokio::test]
    async fn test_mock_storage_get_path_not_found() {
        let storage = MockModelStoragePort::new();
        let result = storage.get_model_path("nonexistent").await;

        assert!(result.is_err());
        match result {
            Err(AppError::NotFound(_)) => {}
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_mock_storage_delete_model() {
        let storage = MockModelStoragePort::with_models();
        assert_eq!(storage.model_count(), 2);

        storage.delete_model("phi-3-mini").await.unwrap();
        assert_eq!(storage.model_count(), 1);

        assert!(!storage.is_model_downloaded("phi-3-mini").await.unwrap());
        assert!(storage.is_model_downloaded("mistral-7b").await.unwrap());
    }

    #[tokio::test]
    async fn test_mock_storage_delete_not_found() {
        let storage = MockModelStoragePort::new();
        let result = storage.delete_model("nonexistent").await;

        assert!(result.is_err());
        match result {
            Err(AppError::NotFound(_)) => {}
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_mock_storage_add_and_clear() {
        let storage = MockModelStoragePort::new();
        assert_eq!(storage.model_count(), 0);

        storage.add_model(DownloadedModel {
            model_id: "test".to_string(),
            path: PathBuf::from("/test"),
            size_bytes: 1000,
            downloaded_at: Utc::now(),
        });
        assert_eq!(storage.model_count(), 1);

        storage.clear();
        assert_eq!(storage.model_count(), 0);
    }
}
