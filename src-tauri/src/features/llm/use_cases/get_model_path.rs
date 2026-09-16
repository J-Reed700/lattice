//! Get Model Path Use Case
//!
//! Retrieves the filesystem path for a downloaded model.
//!
//! # Purpose
//!
//! Gets the canonical filesystem path for a model, ensuring it exists
//! and is downloaded. Returns an error with helpful message if model
//! is not downloaded.
//!
//! # Dependencies
//! - `CheckModelDownloadedUseCase` - Verify model exists
//! - `ModelStoragePort` - Access model paths
//!
//! # Example
//! ```rust,no_run
//! let use_case = GetModelPathUseCase::new(check_downloaded_use_case, model_storage);
//! let request = GetModelPathRequestDto {
//!     model_id: "phi-3-mini".to_string(),
//! };
//! let response = use_case.execute(request).await?;
//! println!("Model path: {}", response.path);
//! ```

use crate::application::ports::model_storage::ModelStoragePort;
use crate::features::llm::dto::{
    CheckModelDownloadedRequestDto, GetModelPathRequestDto, GetModelPathResponseDto,
};
use crate::features::llm::use_cases::CheckModelDownloadedUseCase;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct GetModelPathUseCase {
    check_downloaded: Arc<CheckModelDownloadedUseCase>,
    model_storage: Arc<dyn ModelStoragePort>,
}

impl GetModelPathUseCase {
    pub fn new(
        check_downloaded: Arc<CheckModelDownloadedUseCase>,
        model_storage: Arc<dyn ModelStoragePort>,
    ) -> Self {
        Self {
            check_downloaded,
            model_storage,
        }
    }

    pub async fn execute(
        &self,
        request: GetModelPathRequestDto,
    ) -> Result<GetModelPathResponseDto, AppError> {
        // 1. Check if model is downloaded (also validates model_id)
        let check_request = CheckModelDownloadedRequestDto {
            model_id: request.model_id.clone(),
        };
        let check_response = self.check_downloaded.execute(check_request).await?;

        // 2. If not downloaded, return helpful error
        if !check_response.is_downloaded {
            return Err(AppError::NotFound(format!(
                "Model '{}' is not downloaded. Please download the model first before accessing its path.",
                request.model_id
            )));
        }

        // 3. Get canonical path
        let path = self.model_storage.get_model_path(&request.model_id).await?;

        Ok(GetModelPathResponseDto {
            path: path.to_string_lossy().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::model_storage::MockModelStoragePort;

    fn create_use_case(storage: Arc<MockModelStoragePort>) -> GetModelPathUseCase {
        let check_downloaded = Arc::new(CheckModelDownloadedUseCase::new(storage.clone()));
        GetModelPathUseCase::new(check_downloaded, storage)
    }

    #[tokio::test]
    async fn test_get_path_for_downloaded_model() {
        let storage = Arc::new(MockModelStoragePort::with_models());
        let use_case = create_use_case(storage);

        let request = GetModelPathRequestDto {
            model_id: "phi-3-mini".to_string(),
        };
        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.path, "/models/phi-3-mini");
    }

    #[tokio::test]
    async fn test_get_path_for_another_model() {
        let storage = Arc::new(MockModelStoragePort::with_models());
        let use_case = create_use_case(storage);

        let request = GetModelPathRequestDto {
            model_id: "mistral-7b".to_string(),
        };
        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.path, "/models/mistral-7b");
    }

    #[tokio::test]
    async fn test_model_not_downloaded_returns_error() {
        let storage = Arc::new(MockModelStoragePort::new());
        let use_case = create_use_case(storage);

        let request = GetModelPathRequestDto {
            model_id: "nonexistent".to_string(),
        };
        let result = use_case.execute(request).await;

        assert!(result.is_err());
        match result {
            Err(AppError::NotFound(msg)) => {
                assert!(msg.contains("not downloaded"));
                assert!(msg.contains("nonexistent"));
                assert!(msg.contains("download the model first"));
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_invalid_model_id_returns_error() {
        let storage = Arc::new(MockModelStoragePort::new());
        let use_case = create_use_case(storage);

        let request = GetModelPathRequestDto {
            model_id: "".to_string(),
        };
        let result = use_case.execute(request).await;

        assert!(result.is_err());
        match result {
            Err(AppError::InvalidInput(_)) => {}
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_path_contains_model_id() {
        let storage = Arc::new(MockModelStoragePort::with_models());
        let use_case = create_use_case(storage);

        let model_ids = vec!["phi-3-mini", "mistral-7b"];

        for model_id in model_ids {
            let request = GetModelPathRequestDto {
                model_id: model_id.to_string(),
            };
            let result = use_case.execute(request).await.unwrap();

            assert!(
                result.path.contains(model_id),
                "Path should contain model ID: {} in {}",
                model_id,
                result.path
            );
        }
    }

    #[tokio::test]
    async fn test_error_message_includes_model_id() {
        let storage = Arc::new(MockModelStoragePort::new());
        let use_case = create_use_case(storage);

        let request = GetModelPathRequestDto {
            model_id: "custom-model-123".to_string(),
        };
        let result = use_case.execute(request).await;

        assert!(result.is_err());
        match result {
            Err(AppError::NotFound(msg)) => {
                assert!(msg.contains("custom-model-123"));
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_invalid_characters_rejected() {
        let storage = Arc::new(MockModelStoragePort::new());
        let use_case = create_use_case(storage);

        let request = GetModelPathRequestDto {
            model_id: "../etc/passwd".to_string(),
        };
        let result = use_case.execute(request).await;

        assert!(result.is_err());
        // Should fail validation before checking download status
        match result {
            Err(AppError::InvalidInput(_)) => {}
            _ => panic!("Expected InvalidInput error for path traversal attempt"),
        }
    }

    #[tokio::test]
    async fn test_case_sensitive_model_id() {
        let storage = Arc::new(MockModelStoragePort::with_models());
        let use_case = create_use_case(storage);

        // Exact match should work
        let request = GetModelPathRequestDto {
            model_id: "phi-3-mini".to_string(),
        };
        let result = use_case.execute(request).await;
        assert!(result.is_ok());

        // Case mismatch should not find model
        let request = GetModelPathRequestDto {
            model_id: "PHI-3-MINI".to_string(),
        };
        let result = use_case.execute(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_path_is_non_empty() {
        let storage = Arc::new(MockModelStoragePort::with_models());
        let use_case = create_use_case(storage);

        let request = GetModelPathRequestDto {
            model_id: "phi-3-mini".to_string(),
        };
        let result = use_case.execute(request).await.unwrap();

        assert!(!result.path.is_empty());
        assert!(!result.path.is_empty());
    }
}
