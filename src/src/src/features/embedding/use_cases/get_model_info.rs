//! # Get Embedding Model Info Use Case
//!
//! Retrieves metadata about the current embedding model.
//!
//! This use case orchestrates:
//! 1. Model information retrieval from embedding service
//! 2. Metadata formatting
//!
//! ## Example
//!
//! ```rust,no_run
//! use lattice::application::use_cases::embedding::get_model_info::GetEmbeddingModelInfoUseCase;
//!
//! # async fn example(use_case: GetEmbeddingModelInfoUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let info = use_case.execute().await?;
//! println!("Model: {}", info.model_name);
//! println!("Dimension: {}", info.dimension);
//! println!("Max tokens: {}", info.max_tokens);
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use crate::application::ports::EmbeddingPort;
use crate::domain::embedding_constants::DEFAULT_EMBEDDING_MODEL_NAME;
use crate::features::embedding::dto::EmbeddingModelInfoDto;
use crate::shared::error::Result;

/// Default model name if not specified
const DEFAULT_MODEL_NAME: &str = DEFAULT_EMBEDDING_MODEL_NAME;

/// Default max tokens (typical for DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME)
const DEFAULT_MAX_TOKENS: usize = 512;

/// Get embedding model info use case.
///
/// Retrieves information about the current embedding model:
/// - Model name/identifier
/// - Embedding dimensionality
/// - Maximum token capacity
///
/// ## Dependencies
///
/// - `EmbeddingPort`: Provides model metadata
pub struct GetEmbeddingModelInfoUseCase {
    embedding_service: Arc<dyn EmbeddingPort>,
}

impl GetEmbeddingModelInfoUseCase {
    /// Create a new get embedding model info use case.
    ///
    /// # Arguments
    ///
    /// * `embedding_service` - Service for embedding generation
    pub fn new(embedding_service: Arc<dyn EmbeddingPort>) -> Self {
        Self { embedding_service }
    }

    /// Execute model info retrieval.
    ///
    /// # Returns
    ///
    /// Model metadata including name, dimension, and max tokens
    ///
    /// # Errors
    ///
    /// Returns error if model information cannot be retrieved
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use lattice::application::use_cases::embedding::get_model_info::GetEmbeddingModelInfoUseCase;
    /// # async fn example(use_case: GetEmbeddingModelInfoUseCase) -> Result<(), Box<dyn std::error::Error>> {
    /// let info = use_case.execute().await?;
    ///
    /// println!("Model Information:");
    /// println!("  Name: {}", info.model_name);
    /// println!("  Dimension: {}", info.dimension);
    /// println!("  Max Tokens: {}", info.max_tokens);
    ///
    /// // Use info to validate embeddings
    /// use lattice::domain::embedding_constants::DEFAULT_EMBEDDING_DIM;
    /// if info.dimension != DEFAULT_EMBEDDING_DIM {
    ///     println!("Warning: Unexpected dimension!");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(&self) -> Result<EmbeddingModelInfoDto> {
        // 1. Get dimension from embedding service
        let dimension = self.embedding_service.dimension();

        // 2. Create model info DTO
        // Note: Current EmbeddingPort doesn't expose model name or max_tokens
        // so we use defaults. A future enhancement could add these methods to the port.
        Ok(EmbeddingModelInfoDto {
            model_name: DEFAULT_MODEL_NAME.to_string(),
            dimension,
            max_tokens: DEFAULT_MAX_TOKENS,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::embedding_constants::DEFAULT_EMBEDDING_DIM;
    use crate::shared::error::AppError;
    use async_trait::async_trait;

    /// Mock embedding service for testing
    struct MockEmbeddingService {
        dimension: usize,
    }

    impl MockEmbeddingService {
        fn new(dimension: usize) -> Self {
            Self { dimension }
        }
    }

    #[async_trait]
    impl EmbeddingPort for MockEmbeddingService {
        async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
            unimplemented!("Not used in model info tests")
        }

        async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
            unimplemented!("Not used in model info tests")
        }

        fn dimension(&self) -> usize {
            self.dimension
        }

        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn test_get_model_info_standard_dimension() {
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::new(DEFAULT_EMBEDDING_DIM));
        let use_case = GetEmbeddingModelInfoUseCase::new(mock_service);

        // Act
        let result = use_case.execute().await;

        // Assert
        assert!(result.is_ok());
        let info = result.unwrap();
        assert_eq!(info.dimension, DEFAULT_EMBEDDING_DIM);
        assert_eq!(info.model_name, DEFAULT_MODEL_NAME);
        assert_eq!(info.max_tokens, DEFAULT_MAX_TOKENS);
    }

    #[tokio::test]
    async fn test_get_model_info_large_dimension() {
        // Arrange - Larger model like DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME
        let mock_service = Arc::new(MockEmbeddingService::new(DEFAULT_EMBEDDING_DIM));
        let use_case = GetEmbeddingModelInfoUseCase::new(mock_service);

        // Act
        let result = use_case.execute().await;

        // Assert
        assert!(result.is_ok());
        let info = result.unwrap();
        assert_eq!(info.dimension, DEFAULT_EMBEDDING_DIM);
    }

    #[tokio::test]
    async fn test_get_model_info_openai_dimension() {
        // Arrange - OpenAI text-embedding-3-small
        let mock_service = Arc::new(MockEmbeddingService::new(1536));
        let use_case = GetEmbeddingModelInfoUseCase::new(mock_service);

        // Act
        let result = use_case.execute().await;

        // Assert
        assert!(result.is_ok());
        let info = result.unwrap();
        assert_eq!(info.dimension, 1536);
    }

    #[tokio::test]
    async fn test_get_model_info_returns_all_fields() {
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::new(DEFAULT_EMBEDDING_DIM));
        let use_case = GetEmbeddingModelInfoUseCase::new(mock_service);

        // Act
        let result = use_case.execute().await;

        // Assert
        assert!(result.is_ok());
        let info = result.unwrap();

        // All fields should be populated
        assert!(!info.model_name.is_empty());
        assert!(info.dimension > 0);
        assert!(info.max_tokens > 0);
    }

    #[tokio::test]
    async fn test_get_model_info_multiple_calls_consistent() {
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::new(DEFAULT_EMBEDDING_DIM));
        let use_case = GetEmbeddingModelInfoUseCase::new(mock_service);

        // Act - Call multiple times
        let result1 = use_case.execute().await.unwrap();
        let result2 = use_case.execute().await.unwrap();

        // Assert - Results should be identical
        assert_eq!(result1.dimension, result2.dimension);
        assert_eq!(result1.model_name, result2.model_name);
        assert_eq!(result1.max_tokens, result2.max_tokens);
    }

    #[tokio::test]
    async fn test_get_model_info_dimension_correctness() {
        // Test various common embedding dimensions
        let test_cases = vec![
            512,                   // some custom models
            DEFAULT_EMBEDDING_DIM, // DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME, BERT-base
            1024,                  // some large models
            1536,                  // OpenAI text-embedding-3-small
            3072,                  // OpenAI text-embedding-3-large
        ];

        for dimension in test_cases {
            let mock_service = Arc::new(MockEmbeddingService::new(dimension));
            let use_case = GetEmbeddingModelInfoUseCase::new(mock_service);

            let result = use_case.execute().await;
            assert!(result.is_ok());
            let info = result.unwrap();
            assert_eq!(
                info.dimension, dimension,
                "Dimension should match for {}",
                dimension
            );
        }
    }
}
