//! List Downloaded Models Use Case
//!
//! Lists all locally downloaded models.
//!
//! # Purpose
//!
//! Scans local storage for downloaded LLM models and returns
//! their metadata, sorted by download date (newest first).
//!
//! # Dependencies
//! - `ModelStoragePort` - Local model storage access
//!
//! # Example
//! ```rust,no_run
//! let use_case = ListDownloadedModelsUseCase::new(storage_port);
//! let downloaded = use_case.execute().await?;
//! for model in downloaded.models {
//!     println!("{}: {} GB", model.model_id, model.size_gb);
//! }
//! ```

use crate::application::ports::model_storage::ModelStoragePort;
use crate::features::llm::dto::{DownloadedModelDto, DownloadedModelsDto};
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct ListDownloadedModelsUseCase {
    storage: Arc<dyn ModelStoragePort>,
}

impl ListDownloadedModelsUseCase {
    pub fn new(storage: Arc<dyn ModelStoragePort>) -> Self {
        Self { storage }
    }

    pub async fn execute(&self) -> Result<DownloadedModelsDto, AppError> {
        let mut models = self.storage.list_models().await?;

        // Sort by download date (newest first)
        models.sort_by_key(|model| std::cmp::Reverse(model.downloaded_at));

        // Convert to DTOs
        let model_dtos = models
            .into_iter()
            .map(|m| DownloadedModelDto {
                model_id: m.model_id,
                path: m.path.to_string_lossy().to_string(),
                size_gb: m.size_bytes as f64 / 1_000_000_000.0,
                downloaded_at: m.downloaded_at,
            })
            .collect();

        Ok(DownloadedModelsDto { models: model_dtos })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::model_storage::{DownloadedModel, MockModelStoragePort};
    use chrono::{Duration, Utc};
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_list_models_empty() {
        let mock_storage = Arc::new(MockModelStoragePort::new());
        let use_case = ListDownloadedModelsUseCase::new(mock_storage);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let downloaded = result.unwrap();
        assert_eq!(downloaded.models.len(), 0);
    }

    #[tokio::test]
    async fn test_list_models_with_models() {
        let mock_storage = Arc::new(MockModelStoragePort::with_models());
        let use_case = ListDownloadedModelsUseCase::new(mock_storage);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let downloaded = result.unwrap();
        assert_eq!(downloaded.models.len(), 2);
    }

    #[tokio::test]
    async fn test_list_models_sorted_by_date() {
        let mock_storage = Arc::new(MockModelStoragePort::new());

        // Add models with different download times
        let now = Utc::now();
        mock_storage.add_model(DownloadedModel {
            model_id: "old-model".to_string(),
            path: PathBuf::from("/models/old"),
            size_bytes: 1_000_000_000,
            downloaded_at: now - Duration::days(7),
        });
        mock_storage.add_model(DownloadedModel {
            model_id: "new-model".to_string(),
            path: PathBuf::from("/models/new"),
            size_bytes: 2_000_000_000,
            downloaded_at: now,
        });
        mock_storage.add_model(DownloadedModel {
            model_id: "medium-model".to_string(),
            path: PathBuf::from("/models/medium"),
            size_bytes: 1_500_000_000,
            downloaded_at: now - Duration::days(3),
        });

        let use_case = ListDownloadedModelsUseCase::new(mock_storage);
        let result = use_case.execute().await;

        assert!(result.is_ok());
        let downloaded = result.unwrap();

        // Verify sorted by date (newest first)
        assert_eq!(downloaded.models.len(), 3);
        assert_eq!(downloaded.models[0].model_id, "new-model");
        assert_eq!(downloaded.models[1].model_id, "medium-model");
        assert_eq!(downloaded.models[2].model_id, "old-model");
    }

    #[tokio::test]
    async fn test_list_models_size_conversion() {
        let mock_storage = Arc::new(MockModelStoragePort::new());

        // Add model with specific byte size
        mock_storage.add_model(DownloadedModel {
            model_id: "test-model".to_string(),
            path: PathBuf::from("/models/test"),
            size_bytes: 1_800_000_000, // 1.8 GB
            downloaded_at: Utc::now(),
        });

        let use_case = ListDownloadedModelsUseCase::new(mock_storage);
        let result = use_case.execute().await;

        assert!(result.is_ok());
        let downloaded = result.unwrap();
        assert_eq!(downloaded.models.len(), 1);

        // Verify size conversion from bytes to GB
        let model = &downloaded.models[0];
        assert_eq!(model.size_gb, 1.8);
    }

    #[tokio::test]
    async fn test_list_models_path_conversion() {
        let mock_storage = Arc::new(MockModelStoragePort::new());

        mock_storage.add_model(DownloadedModel {
            model_id: "path-test".to_string(),
            path: PathBuf::from("/models/path-test"),
            size_bytes: 1_000_000_000,
            downloaded_at: Utc::now(),
        });

        let use_case = ListDownloadedModelsUseCase::new(mock_storage);
        let result = use_case.execute().await;

        assert!(result.is_ok());
        let downloaded = result.unwrap();
        assert_eq!(downloaded.models.len(), 1);

        // Verify path is converted to string
        let model = &downloaded.models[0];
        assert_eq!(model.path, "/models/path-test");
    }

    #[tokio::test]
    async fn test_list_models_dto_fields() {
        let mock_storage = Arc::new(MockModelStoragePort::new());

        let download_time = Utc::now();
        mock_storage.add_model(DownloadedModel {
            model_id: "dto-test".to_string(),
            path: PathBuf::from("/models/dto-test"),
            size_bytes: 4_100_000_000, // 4.1 GB
            downloaded_at: download_time,
        });

        let use_case = ListDownloadedModelsUseCase::new(mock_storage);
        let result = use_case.execute().await;

        assert!(result.is_ok());
        let downloaded = result.unwrap();
        assert_eq!(downloaded.models.len(), 1);

        let model = &downloaded.models[0];
        assert_eq!(model.model_id, "dto-test");
        assert_eq!(model.path, "/models/dto-test");
        assert_eq!(model.size_gb, 4.1);
        assert_eq!(model.downloaded_at, download_time);
    }

    #[tokio::test]
    async fn test_list_models_multiple_same_date() {
        let mock_storage = Arc::new(MockModelStoragePort::new());

        let same_time = Utc::now();
        mock_storage.add_model(DownloadedModel {
            model_id: "model-a".to_string(),
            path: PathBuf::from("/models/a"),
            size_bytes: 1_000_000_000,
            downloaded_at: same_time,
        });
        mock_storage.add_model(DownloadedModel {
            model_id: "model-b".to_string(),
            path: PathBuf::from("/models/b"),
            size_bytes: 2_000_000_000,
            downloaded_at: same_time,
        });

        let use_case = ListDownloadedModelsUseCase::new(mock_storage);
        let result = use_case.execute().await;

        assert!(result.is_ok());
        let downloaded = result.unwrap();
        assert_eq!(downloaded.models.len(), 2);

        // Both should have same download time
        assert_eq!(downloaded.models[0].downloaded_at, same_time);
        assert_eq!(downloaded.models[1].downloaded_at, same_time);
    }

    #[tokio::test]
    async fn test_list_models_large_size() {
        let mock_storage = Arc::new(MockModelStoragePort::new());

        // Add very large model (26 GB)
        mock_storage.add_model(DownloadedModel {
            model_id: "large-model".to_string(),
            path: PathBuf::from("/models/large"),
            size_bytes: 26_000_000_000, // 26 GB
            downloaded_at: Utc::now(),
        });

        let use_case = ListDownloadedModelsUseCase::new(mock_storage);
        let result = use_case.execute().await;

        assert!(result.is_ok());
        let downloaded = result.unwrap();
        assert_eq!(downloaded.models.len(), 1);

        let model = &downloaded.models[0];
        assert_eq!(model.size_gb, 26.0);
    }

    #[tokio::test]
    async fn test_list_models_small_size() {
        let mock_storage = Arc::new(MockModelStoragePort::new());

        // Add small model (1.1 GB)
        mock_storage.add_model(DownloadedModel {
            model_id: "small-model".to_string(),
            path: PathBuf::from("/models/small"),
            size_bytes: 1_100_000_000, // 1.1 GB
            downloaded_at: Utc::now(),
        });

        let use_case = ListDownloadedModelsUseCase::new(mock_storage);
        let result = use_case.execute().await;

        assert!(result.is_ok());
        let downloaded = result.unwrap();
        assert_eq!(downloaded.models.len(), 1);

        let model = &downloaded.models[0];
        assert_eq!(model.size_gb, 1.1);
    }
}
