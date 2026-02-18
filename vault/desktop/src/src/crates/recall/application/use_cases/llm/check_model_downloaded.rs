//! Check Model Downloaded Use Case
//!
//! Verifies if a model exists locally and is valid.
//!
//! # Purpose
//!
//! Checks if a model has been downloaded to the local filesystem
//! and validates its integrity. Returns download status and path.
//!
//! # Dependencies
//! - `ModelStoragePort` - Access local model storage
//!
//! # Example
//! ```rust,no_run
//! let use_case = CheckModelDownloadedUseCase::new(model_storage);
//! let request = CheckModelDownloadedRequestDto {
//!     model_id: "phi-3-mini".to_string(),
//! };
//! let response = use_case.execute(request).await?;
//! if response.is_downloaded {
//!     println!("Model found at: {:?}", response.local_path);
//! }
//! ```

use crate::application::dtos::llm_dto::{
    CheckModelDownloadedRequestDto, CheckModelDownloadedResponseDto,
};
use crate::application::ports::model_storage::ModelStoragePort;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct CheckModelDownloadedUseCase {
    model_storage: Arc<dyn ModelStoragePort>,
}

impl CheckModelDownloadedUseCase {
    pub fn new(model_storage: Arc<dyn ModelStoragePort>) -> Self {
        Self { model_storage }
    }

    pub async fn execute(
        &self,
        request: CheckModelDownloadedRequestDto,
    ) -> Result<CheckModelDownloadedResponseDto, AppError> {
        // 1. Validate model_id
        Self::validate_model_id(&request.model_id)?;

        // 2. Check if model is downloaded
        let is_downloaded = self
            .model_storage
            .is_model_downloaded(&request.model_id)
            .await?;

        // 3. Get path if downloaded
        let local_path = if is_downloaded {
            match self.model_storage.get_model_path(&request.model_id).await {
                Ok(path) => Some(path.to_string_lossy().to_string()),
                Err(_) => None, // Path not found, mark as not downloaded
            }
        } else {
            None
        };

        Ok(CheckModelDownloadedResponseDto {
            is_downloaded: local_path.is_some(),
            local_path,
        })
    }

    /// Validate model_id format.
    ///
    /// Rules:
    /// - Must be non-empty
    /// - Must contain only alphanumeric characters, hyphens, underscores, and dots
    /// - Must not start or end with special characters
    fn validate_model_id(model_id: &str) -> Result<(), AppError> {
        if model_id.is_empty() {
            return Err(AppError::InvalidInput(
                "Model ID cannot be empty".to_string(),
            ));
        }

        // Check for invalid characters
        if !model_id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            return Err(AppError::InvalidInput(
                "Model ID can only contain alphanumeric characters, hyphens, underscores, and dots"
                    .to_string(),
            ));
        }

        // Check for leading/trailing special characters
        let first_char = match model_id.chars().next() {
            Some(c) => c,
            None => {
                return Err(AppError::InvalidInput(
                    "Model ID cannot be empty".to_string(),
                ))
            }
        };
        let last_char = match model_id.chars().last() {
            Some(c) => c,
            None => {
                return Err(AppError::InvalidInput(
                    "Model ID cannot be empty".to_string(),
                ))
            }
        };
        if !first_char.is_alphanumeric() || !last_char.is_alphanumeric() {
            return Err(AppError::InvalidInput(
                "Model ID must start and end with alphanumeric characters".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::model_storage::MockModelStoragePort;

    #[tokio::test]
    async fn test_model_exists_and_valid() {
        let storage = Arc::new(MockModelStoragePort::with_models());
        let use_case = CheckModelDownloadedUseCase::new(storage);

        let request = CheckModelDownloadedRequestDto {
            model_id: "phi-3-mini".to_string(),
        };
        let result = use_case.execute(request).await.unwrap();

        assert!(result.is_downloaded);
        assert!(result.local_path.is_some());
        assert_eq!(result.local_path.unwrap(), "/models/phi-3-mini");
    }

    #[tokio::test]
    async fn test_model_does_not_exist() {
        let storage = Arc::new(MockModelStoragePort::new());
        let use_case = CheckModelDownloadedUseCase::new(storage);

        let request = CheckModelDownloadedRequestDto {
            model_id: "nonexistent".to_string(),
        };
        let result = use_case.execute(request).await.unwrap();

        assert!(!result.is_downloaded);
        assert!(result.local_path.is_none());
    }

    #[tokio::test]
    async fn test_multiple_models() {
        let storage = Arc::new(MockModelStoragePort::with_models());
        let use_case = CheckModelDownloadedUseCase::new(storage);

        // Check phi-3-mini
        let request1 = CheckModelDownloadedRequestDto {
            model_id: "phi-3-mini".to_string(),
        };
        let result1 = use_case.execute(request1).await.unwrap();
        assert!(result1.is_downloaded);

        // Check mistral-7b
        let request2 = CheckModelDownloadedRequestDto {
            model_id: "mistral-7b".to_string(),
        };
        let result2 = use_case.execute(request2).await.unwrap();
        assert!(result2.is_downloaded);

        // Check nonexistent
        let request3 = CheckModelDownloadedRequestDto {
            model_id: "nonexistent".to_string(),
        };
        let result3 = use_case.execute(request3).await.unwrap();
        assert!(!result3.is_downloaded);
    }

    #[tokio::test]
    async fn test_empty_model_id_returns_error() {
        let storage = Arc::new(MockModelStoragePort::new());
        let use_case = CheckModelDownloadedUseCase::new(storage);

        let request = CheckModelDownloadedRequestDto {
            model_id: "".to_string(),
        };
        let result = use_case.execute(request).await;

        assert!(result.is_err());
        match result {
            Err(AppError::InvalidInput(msg)) => {
                assert!(msg.contains("cannot be empty"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_invalid_model_id_with_special_chars() {
        let storage = Arc::new(MockModelStoragePort::new());
        let use_case = CheckModelDownloadedUseCase::new(storage);

        let invalid_ids = vec![
            "model@123",      // @ not allowed
            "model#name",     // # not allowed
            "model name",     // space not allowed
            "model/path",     // / not allowed
            "../traversal",   // / not allowed
            "model\\windows", // \ not allowed
        ];

        for invalid_id in invalid_ids {
            let request = CheckModelDownloadedRequestDto {
                model_id: invalid_id.to_string(),
            };
            let result = use_case.execute(request).await;

            assert!(result.is_err(), "Should reject invalid ID: {}", invalid_id);
            match result {
                Err(AppError::InvalidInput(_)) => {}
                _ => panic!("Expected InvalidInput error for: {}", invalid_id),
            }
        }
    }

    #[tokio::test]
    async fn test_valid_model_id_formats() {
        let storage = Arc::new(MockModelStoragePort::new());
        let use_case = CheckModelDownloadedUseCase::new(storage);

        let valid_ids = vec![
            "phi-3-mini",
            "llama-3.1-13b",
            "mistral_7b",
            "model123",
            "GPT_4",
            "phi-3.5-mini_v2",
        ];

        for valid_id in valid_ids {
            let request = CheckModelDownloadedRequestDto {
                model_id: valid_id.to_string(),
            };
            let result = use_case.execute(request).await;

            assert!(result.is_ok(), "Should accept valid ID: {}", valid_id);
        }
    }

    #[tokio::test]
    async fn test_model_id_starting_with_special_char() {
        let storage = Arc::new(MockModelStoragePort::new());
        let use_case = CheckModelDownloadedUseCase::new(storage);

        let request = CheckModelDownloadedRequestDto {
            model_id: "-model".to_string(),
        };
        let result = use_case.execute(request).await;

        assert!(result.is_err());
        match result {
            Err(AppError::InvalidInput(msg)) => {
                assert!(msg.contains("start and end"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_model_id_ending_with_special_char() {
        let storage = Arc::new(MockModelStoragePort::new());
        let use_case = CheckModelDownloadedUseCase::new(storage);

        let request = CheckModelDownloadedRequestDto {
            model_id: "model-".to_string(),
        };
        let result = use_case.execute(request).await;

        assert!(result.is_err());
        match result {
            Err(AppError::InvalidInput(msg)) => {
                assert!(msg.contains("start and end"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_path_format() {
        let storage = Arc::new(MockModelStoragePort::with_models());
        let use_case = CheckModelDownloadedUseCase::new(storage);

        let request = CheckModelDownloadedRequestDto {
            model_id: "phi-3-mini".to_string(),
        };
        let result = use_case.execute(request).await.unwrap();

        assert!(result.is_downloaded);
        let path = result.local_path.unwrap();
        assert!(path.contains("phi-3-mini"));
    }
}
