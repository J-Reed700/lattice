//! # Generate Single Embedding Use Case
//!
//! Generates a vector embedding for a single text input.
//!
//! This use case orchestrates:
//! 1. Text validation (non-empty, length limit)
//! 2. Text normalization
//! 3. Embedding generation via ONNX model
//! 4. Dimension validation
//!
//! ## Example
//!
//! ```rust,no_run
//! use lattice::application::use_cases::embedding::generate_single_embedding::GenerateSingleEmbeddingUseCase;
//! use lattice::application::dtos::embedding_dto::GenerateSingleEmbeddingRequestDto;
//!
//! # async fn example(use_case: GenerateSingleEmbeddingUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = GenerateSingleEmbeddingRequestDto {
//!     text: "This is the text to embed".to_string(),
//! };
//!
//! let response = use_case.execute(request).await?;
//! println!("Embedding dimension: {}", response.dimension);
//! println!("Embedding vector length: {}", response.embedding.len());
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use crate::application::ports::EmbeddingPort;
use crate::features::embedding::dto::{
    GenerateSingleEmbeddingRequestDto, GenerateSingleEmbeddingResponseDto,
};
use crate::shared::error::{AppError, Result};

/// Maximum text length for embedding (10,000 characters)
const MAX_TEXT_LENGTH: usize = 10_000;

/// Generate single embedding use case.
///
/// Generates a vector embedding for a single text input:
/// - Validates text length
/// - Normalizes text
/// - Generates embedding via model
///
/// ## Dependencies
///
/// - `EmbeddingPort`: Handles embedding generation (ONNX, OpenAI, etc.)
pub struct GenerateSingleEmbeddingUseCase {
    embedding_service: Arc<dyn EmbeddingPort>,
}

impl GenerateSingleEmbeddingUseCase {
    /// Create a new generate single embedding use case.
    ///
    /// # Arguments
    ///
    /// * `embedding_service` - Service for embedding generation
    pub fn new(embedding_service: Arc<dyn EmbeddingPort>) -> Self {
        Self { embedding_service }
    }

    /// Execute single embedding generation.
    ///
    /// # Arguments
    ///
    /// * `request` - Request containing the text to embed
    ///
    /// # Returns
    ///
    /// Response with embedding vector and dimension
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Text is empty
    /// - Text exceeds max length (10,000 characters)
    /// - Embedding generation fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use lattice::application::use_cases::embedding::generate_single_embedding::GenerateSingleEmbeddingUseCase;
    /// # use lattice::application::dtos::embedding_dto::GenerateSingleEmbeddingRequestDto;
    /// # async fn example(use_case: GenerateSingleEmbeddingUseCase) -> Result<(), Box<dyn std::error::Error>> {
    /// let request = GenerateSingleEmbeddingRequestDto {
    ///     text: "Sample document for embedding generation".to_string(),
    /// };
    ///
    /// match use_case.execute(request).await {
    ///     Ok(response) => {
    ///         println!("Success!");
    ///         println!("  Dimension: {}", response.dimension);
    ///         println!("  Vector length: {}", response.embedding.len());
    ///         println!("  First value: {}", response.embedding[0]);
    ///     }
    ///     Err(e) => eprintln!("Failed: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// ```
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
    use crate::domain::embedding_constants::DEFAULT_EMBEDDING_DIM;
    use async_trait::async_trait;

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

            // Generate a simple mock embedding based on text length
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
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateSingleEmbeddingUseCase::new(mock_service);

        // Act
        let result = use_case
            .execute(GenerateSingleEmbeddingRequestDto {
                text: "This is a test document for embedding".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.dimension, 384);
        assert_eq!(response.embedding.len(), 384);
    }

    #[tokio::test]
    async fn test_generate_embedding_with_different_dimension() {
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::new(DEFAULT_EMBEDDING_DIM));
        let use_case = GenerateSingleEmbeddingUseCase::new(mock_service);

        // Act
        let result = use_case
            .execute(GenerateSingleEmbeddingRequestDto {
                text: "Test text".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.dimension, DEFAULT_EMBEDDING_DIM);
        assert_eq!(response.embedding.len(), DEFAULT_EMBEDDING_DIM);
    }

    #[tokio::test]
    async fn test_generate_embedding_empty_text() {
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateSingleEmbeddingUseCase::new(mock_service);

        // Act
        let result = use_case
            .execute(GenerateSingleEmbeddingRequestDto {
                text: "".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => assert!(msg.contains("empty")),
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_generate_embedding_whitespace_only() {
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateSingleEmbeddingUseCase::new(mock_service);

        // Act
        let result = use_case
            .execute(GenerateSingleEmbeddingRequestDto {
                text: "   \n\t   ".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => assert!(msg.contains("empty")),
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_generate_embedding_text_too_long() {
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateSingleEmbeddingUseCase::new(mock_service);

        // Create text that exceeds max length
        let long_text = "a".repeat(MAX_TEXT_LENGTH + 1);

        // Act
        let result = use_case
            .execute(GenerateSingleEmbeddingRequestDto { text: long_text })
            .await;

        // Assert
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
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateSingleEmbeddingUseCase::new(mock_service);

        // Create text exactly at max length
        let max_text = "a".repeat(MAX_TEXT_LENGTH);

        // Act
        let result = use_case
            .execute(GenerateSingleEmbeddingRequestDto { text: max_text })
            .await;

        // Assert
        assert!(result.is_ok()); // Should succeed at exactly max length
    }

    #[tokio::test]
    async fn test_generate_embedding_model_error() {
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::with_failure());
        let use_case = GenerateSingleEmbeddingUseCase::new(mock_service);

        // Act
        let result = use_case
            .execute(GenerateSingleEmbeddingRequestDto {
                text: "Test text".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::EmbeddingFailed { reason } => assert_eq!(reason, "Model error"),
            _ => panic!("Expected EmbeddingFailed error"),
        }
    }

    #[tokio::test]
    async fn test_generate_embedding_text_normalization() {
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateSingleEmbeddingUseCase::new(mock_service);

        // Act - text with leading/trailing whitespace should be trimmed
        let result = use_case
            .execute(GenerateSingleEmbeddingRequestDto {
                text: "  Test text with whitespace  \n".to_string(),
            })
            .await;

        // Assert
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.dimension, 384);
    }
}
