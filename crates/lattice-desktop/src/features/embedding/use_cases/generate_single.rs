//! Generates a vector embedding for a single text input.

use std::sync::Arc;

use crate::features::embedding::dto::{
    GenerateSingleEmbeddingRequestDto, GenerateSingleEmbeddingResponseDto,
};
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::shared::error::{AppError, Result};

/// Maximum text length for embedding (10,000 characters)
const MAX_TEXT_LENGTH: usize = 10_000;

/// Generate single embedding use case.
pub struct GenerateSingleEmbeddingUseCase {
    embedding_service: Arc<dyn EmbeddingPort>,
}

impl GenerateSingleEmbeddingUseCase {
    pub fn new(embedding_service: Arc<dyn EmbeddingPort>) -> Self {
        Self { embedding_service }
    }

    /// Execute single embedding generation.
    pub async fn execute(
        &self,
        request: GenerateSingleEmbeddingRequestDto,
    ) -> Result<GenerateSingleEmbeddingResponseDto> {
        // 1. Validate text is not empty
        if request.text.trim().is_empty() {
            return Err(AppError::InvalidInput("Text cannot be empty".to_string()));
        }

        // 2. Validate text length
        if request.text.len() > MAX_TEXT_LENGTH {
            return Err(AppError::InvalidInput(format!(
                "Text exceeds maximum length of {} characters (got {} characters)",
                MAX_TEXT_LENGTH,
                request.text.len()
            )));
        }

        // 3. Normalize text (trim whitespace)
        let normalized_text = request.text.trim();

        // 4. Generate embedding via port
        let embedding = self.embedding_service.embed_single(normalized_text).await?;

        // 5. Get dimension from service
        let dimension = self.embedding_service.dimension();

        // 6. Validate embedding dimension matches
        if embedding.len() != dimension {
            return Err(AppError::EmbeddingFailed {
                reason: format!(
                    "Embedding dimension mismatch: expected {}, got {}",
                    dimension,
                    embedding.len()
                ),
            });
        }

        // 7. Return response
        Ok(GenerateSingleEmbeddingResponseDto {
            embedding,
            dimension,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use lattice_core::domain::models::embedding_defaults::DEFAULT_EMBEDDING_DIM;

    /// Mock embedding service for testing
    struct MockEmbeddingService {
        dimension: usize,
        should_fail: bool,
    }

    impl MockEmbeddingService {
        fn new(dimension: usize) -> Self {
            Self {
                dimension,
                should_fail: false,
            }
        }

        fn with_failure() -> Self {
            Self {
                dimension: 384,
                should_fail: true,
            }
        }
    }

    #[async_trait]
    impl EmbeddingPort for MockEmbeddingService {
        async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
            if self.should_fail {
                return Err(AppError::EmbeddingFailed {
                    reason: "Model error".to_string(),
                });
            }

            let value = (text.len() as f32) / 100.0;
            Ok(vec![value; self.dimension])
        }

        async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
            unimplemented!("Not used in this test")
        }

        fn dimension(&self) -> usize {
            self.dimension
        }

        async fn is_ready(&self) -> Result<bool> {
            Ok(!self.should_fail)
        }
    }

    #[tokio::test]
    async fn test_generate_single_embedding_success() {
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateSingleEmbeddingUseCase::new(mock_service);

        let result = use_case
            .execute(GenerateSingleEmbeddingRequestDto {
                text: "This is a test document for embedding".to_string(),
            })
            .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.dimension, 384);
        assert_eq!(response.embedding.len(), 384);
    }

    #[tokio::test]
    async fn test_generate_embedding_with_different_dimension() {
        let mock_service = Arc::new(MockEmbeddingService::new(DEFAULT_EMBEDDING_DIM));
        let use_case = GenerateSingleEmbeddingUseCase::new(mock_service);

        let result = use_case
            .execute(GenerateSingleEmbeddingRequestDto {
                text: "Test text".to_string(),
            })
            .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.dimension, DEFAULT_EMBEDDING_DIM);
        assert_eq!(response.embedding.len(), DEFAULT_EMBEDDING_DIM);
    }

    #[tokio::test]
    async fn test_generate_embedding_empty_text() {
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateSingleEmbeddingUseCase::new(mock_service);

        let result = use_case
            .execute(GenerateSingleEmbeddingRequestDto {
                text: "".to_string(),
            })
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => assert!(msg.contains("empty")),
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_generate_embedding_whitespace_only() {
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateSingleEmbeddingUseCase::new(mock_service);

        let result = use_case
            .execute(GenerateSingleEmbeddingRequestDto {
                text: "   \n\t   ".to_string(),
            })
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => assert!(msg.contains("empty")),
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_generate_embedding_text_too_long() {
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateSingleEmbeddingUseCase::new(mock_service);

        let long_text = "a".repeat(MAX_TEXT_LENGTH + 1);

        let result = use_case
            .execute(GenerateSingleEmbeddingRequestDto { text: long_text })
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => {
                assert!(msg.contains("exceeds maximum length"));
                assert!(msg.contains(&MAX_TEXT_LENGTH.to_string()));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_generate_embedding_at_max_length() {
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateSingleEmbeddingUseCase::new(mock_service);

        let max_text = "a".repeat(MAX_TEXT_LENGTH);

        let result = use_case
            .execute(GenerateSingleEmbeddingRequestDto { text: max_text })
            .await;

        assert!(result.is_ok()); // Should succeed at exactly max length
    }

    #[tokio::test]
    async fn test_generate_embedding_model_error() {
        let mock_service = Arc::new(MockEmbeddingService::with_failure());
        let use_case = GenerateSingleEmbeddingUseCase::new(mock_service);

        let result = use_case
            .execute(GenerateSingleEmbeddingRequestDto {
                text: "Test text".to_string(),
            })
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::EmbeddingFailed { reason } => assert_eq!(reason, "Model error"),
            _ => panic!("Expected EmbeddingFailed error"),
        }
    }

    #[tokio::test]
    async fn test_generate_embedding_text_normalization() {
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateSingleEmbeddingUseCase::new(mock_service);

        let result = use_case
            .execute(GenerateSingleEmbeddingRequestDto {
                text: "  Test text with whitespace  \n".to_string(),
            })
            .await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.dimension, 384);
    }
}
