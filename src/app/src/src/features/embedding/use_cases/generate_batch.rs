//! # Generate Batch Embeddings Use Case
//!
//! Generates vector embeddings for multiple text inputs efficiently.
//!
//! This use case orchestrates:
//! 1. Batch validation (size limits)
//! 2. Individual text validation (non-empty, length limits)
//! 3. Batch embedding generation (optimized for GPU/remote APIs)
//! 4. Order preservation (output matches input order)
//!
//! ## Example
//!
//! ```rust,no_run
//! use vault_desktop::application::use_cases::embedding::generate_batch_embeddings::GenerateBatchEmbeddingsUseCase;
//! use vault_desktop::application::dtos::embedding_dto::GenerateBatchEmbeddingsRequestDto;
//!
//! # async fn example(use_case: GenerateBatchEmbeddingsUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let request = GenerateBatchEmbeddingsRequestDto {
//!     texts: vec![
//!         "First document".to_string(),
//!         "Second document".to_string(),
//!         "Third document".to_string(),
//!     ],
//! };
//!
//! let response = use_case.execute(request).await?;
//! println!("Generated {} embeddings", response.embeddings.len());
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use crate::features::embedding::dto::{
    GenerateBatchEmbeddingsRequestDto, GenerateBatchEmbeddingsResponseDto,
};
use crate::application::ports::EmbeddingPort;
use crate::shared::error::{AppError, Result};

/// Maximum batch size (100 items)
const MAX_BATCH_SIZE: usize = 100;

/// Minimum batch size (1 item)
const MIN_BATCH_SIZE: usize = 1;

/// Maximum text length per item (10,000 characters)
const MAX_TEXT_LENGTH: usize = 10_000;

/// Generate batch embeddings use case.
///
/// Generates embeddings for multiple texts in a single batch:
/// - More efficient than repeated single calls
/// - Optimized for GPU/remote API batching
/// - Preserves input order
///
/// ## Dependencies
///
/// - `EmbeddingPort`: Handles batch embedding generation
pub struct GenerateBatchEmbeddingsUseCase {
    embedding_service: Arc<dyn EmbeddingPort>,
}

impl GenerateBatchEmbeddingsUseCase {
    /// Create a new generate batch embeddings use case.
    ///
    /// # Arguments
    ///
    /// * `embedding_service` - Service for embedding generation
    pub fn new(embedding_service: Arc<dyn EmbeddingPort>) -> Self {
        Self { embedding_service }
    }

    /// Execute batch embedding generation.
    ///
    /// # Arguments
    ///
    /// * `request` - Request containing texts to embed (1-100 items)
    ///
    /// # Returns
    ///
    /// Response with embeddings in the same order as input
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Batch is empty
    /// - Batch exceeds max size (100 items)
    /// - Any text is empty
    /// - Any text exceeds max length (10,000 characters)
    /// - Embedding generation fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use vault_desktop::application::use_cases::embedding::generate_batch_embeddings::GenerateBatchEmbeddingsUseCase;
    /// # use vault_desktop::application::dtos::embedding_dto::GenerateBatchEmbeddingsRequestDto;
    /// # async fn example(use_case: GenerateBatchEmbeddingsUseCase) -> Result<(), Box<dyn std::error::Error>> {
    /// let texts = vec![
    ///     "Document about machine learning".to_string(),
    ///     "Article on deep learning".to_string(),
    ///     "Paper about neural networks".to_string(),
    /// ];
    ///
    /// let request = GenerateBatchEmbeddingsRequestDto { texts };
    ///
    /// match use_case.execute(request).await {
    ///     Ok(response) => {
    ///         println!("Generated {} embeddings", response.embeddings.len());
    ///         for (i, emb) in response.embeddings.iter().enumerate() {
    ///             println!("  Embedding {}: {} dimensions", i, emb.len());
    ///         }
    ///     }
    ///     Err(e) => eprintln!("Failed: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(
        &self,
        request: GenerateBatchEmbeddingsRequestDto,
    ) -> Result<GenerateBatchEmbeddingsResponseDto> {
        // 1. Validate batch size
        let batch_size = request.texts.len();

        if batch_size < MIN_BATCH_SIZE {
            return Err(AppError::InvalidInput("Batch cannot be empty".to_string()));
        }

        if batch_size > MAX_BATCH_SIZE {
            return Err(AppError::InvalidInput(format!(
                "Batch size exceeds maximum of {} items (got {} items)",
                MAX_BATCH_SIZE, batch_size
            )));
        }

        // 2. Validate each text
        for (i, text) in request.texts.iter().enumerate() {
            // Check empty
            if text.trim().is_empty() {
                return Err(AppError::InvalidInput(format!(
                    "Text at index {} is empty",
                    i
                )));
            }

            // Check length
            if text.len() > MAX_TEXT_LENGTH {
                return Err(AppError::InvalidInput(format!(
                    "Text at index {} exceeds maximum length of {} characters (got {} characters)",
                    i,
                    MAX_TEXT_LENGTH,
                    text.len()
                )));
            }
        }

        // 3. Normalize texts (trim whitespace)
        let normalized_texts: Vec<String> =
            request.texts.iter().map(|t| t.trim().to_string()).collect();

        // 4. Generate embeddings via port
        let embeddings = self
            .embedding_service
            .embed_batch(&normalized_texts)
            .await?;

        // 5. Validate response
        if embeddings.len() != batch_size {
            return Err(AppError::EmbeddingFailed {
                reason: format!(
                    "Batch embedding response size mismatch: expected {}, got {}",
                    batch_size,
                    embeddings.len()
                ),
            });
        }

        // 6. Return response
        Ok(GenerateBatchEmbeddingsResponseDto { embeddings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
            unimplemented!("Not used in batch tests")
        }

        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            if self.should_fail {
                return Err(AppError::EmbeddingFailed {
                    reason: "Batch model error".to_string(),
                });
            }

            // Generate mock embeddings
            let embeddings = texts
                .iter()
                .map(|text| {
                    let value = (text.len() as f32) / 100.0;
                    vec![value; self.dimension]
                })
                .collect();

            Ok(embeddings)
        }

        fn dimension(&self) -> usize {
            self.dimension
        }

        async fn is_ready(&self) -> Result<bool> {
            Ok(!self.should_fail)
        }
    }

    #[tokio::test]
    async fn test_generate_batch_embeddings_small_batch() {
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateBatchEmbeddingsUseCase::new(mock_service);

        let texts = vec![
            "First document".to_string(),
            "Second document".to_string(),
            "Third document".to_string(),
        ];

        // Act
        let result = use_case
            .execute(GenerateBatchEmbeddingsRequestDto { texts })
            .await;

        // Assert
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.embeddings.len(), 3);
        assert_eq!(response.embeddings[0].len(), 384);
        assert_eq!(response.embeddings[1].len(), 384);
        assert_eq!(response.embeddings[2].len(), 384);
    }

    #[tokio::test]
    async fn test_generate_batch_embeddings_large_batch() {
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateBatchEmbeddingsUseCase::new(mock_service);

        // Create max batch size
        let texts: Vec<String> = (0..MAX_BATCH_SIZE)
            .map(|i| format!("Document number {}", i))
            .collect();

        // Act
        let result = use_case
            .execute(GenerateBatchEmbeddingsRequestDto { texts })
            .await;

        // Assert
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.embeddings.len(), MAX_BATCH_SIZE);
    }

    #[tokio::test]
    async fn test_generate_batch_embeddings_too_large() {
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateBatchEmbeddingsUseCase::new(mock_service);

        // Exceed max batch size
        let texts: Vec<String> = (0..MAX_BATCH_SIZE + 1)
            .map(|i| format!("Document {}", i))
            .collect();

        // Act
        let result = use_case
            .execute(GenerateBatchEmbeddingsRequestDto { texts })
            .await;

        // Assert
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => {
                assert!(msg.contains("exceeds maximum"));
                assert!(msg.contains(&MAX_BATCH_SIZE.to_string()));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_generate_batch_embeddings_empty_batch() {
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateBatchEmbeddingsUseCase::new(mock_service);

        // Act
        let result = use_case
            .execute(GenerateBatchEmbeddingsRequestDto { texts: vec![] })
            .await;

        // Assert
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => assert!(msg.contains("empty")),
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_generate_batch_embeddings_contains_empty_text() {
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateBatchEmbeddingsUseCase::new(mock_service);

        let texts = vec![
            "Valid text".to_string(),
            "".to_string(), // Empty!
            "Another valid text".to_string(),
        ];

        // Act
        let result = use_case
            .execute(GenerateBatchEmbeddingsRequestDto { texts })
            .await;

        // Assert
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => {
                assert!(msg.contains("index 1"));
                assert!(msg.contains("empty"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_generate_batch_embeddings_contains_too_long_text() {
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateBatchEmbeddingsUseCase::new(mock_service);

        let texts = vec![
            "Valid text".to_string(),
            "a".repeat(MAX_TEXT_LENGTH + 1), // Too long!
            "Another valid text".to_string(),
        ];

        // Act
        let result = use_case
            .execute(GenerateBatchEmbeddingsRequestDto { texts })
            .await;

        // Assert
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => {
                assert!(msg.contains("index 1"));
                assert!(msg.contains("exceeds maximum length"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_generate_batch_embeddings_model_error() {
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::with_failure());
        let use_case = GenerateBatchEmbeddingsUseCase::new(mock_service);

        let texts = vec!["Text 1".to_string(), "Text 2".to_string()];

        // Act
        let result = use_case
            .execute(GenerateBatchEmbeddingsRequestDto { texts })
            .await;

        // Assert
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::EmbeddingFailed { reason } => assert_eq!(reason, "Batch model error"),
            _ => panic!("Expected EmbeddingFailed error"),
        }
    }

    #[tokio::test]
    async fn test_generate_batch_embeddings_order_preserved() {
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateBatchEmbeddingsUseCase::new(mock_service);

        let texts = vec![
            "Short".to_string(),
            "Medium length text".to_string(),
            "Very long text with many words".to_string(),
        ];

        // Act
        let result = use_case
            .execute(GenerateBatchEmbeddingsRequestDto {
                texts: texts.clone(),
            })
            .await;

        // Assert
        assert!(result.is_ok());
        let response = result.unwrap();

        // Embeddings should correspond to text lengths (in our mock)
        // Short text → small value, long text → larger value
        assert!(response.embeddings[0][0] < response.embeddings[1][0]);
        assert!(response.embeddings[1][0] < response.embeddings[2][0]);
    }

    #[tokio::test]
    async fn test_generate_batch_embeddings_single_item() {
        // Arrange
        let mock_service = Arc::new(MockEmbeddingService::new(384));
        let use_case = GenerateBatchEmbeddingsUseCase::new(mock_service);

        let texts = vec!["Single document".to_string()];

        // Act
        let result = use_case
            .execute(GenerateBatchEmbeddingsRequestDto { texts })
            .await;

        // Assert
        assert!(result.is_ok()); // Batch of 1 is valid
        let response = result.unwrap();
        assert_eq!(response.embeddings.len(), 1);
    }
}
