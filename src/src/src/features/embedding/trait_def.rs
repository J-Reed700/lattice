//! Service trait definitions
//!
//! This module defines trait interfaces for dependency injection.

use crate::shared::error::Result;
use async_trait::async_trait;

#[async_trait]
pub trait EmbeddingServiceTrait: Send + Sync {
    /// Generate embedding for a single text string
    ///
    /// # Arguments
    /// * `text` - The text to embed
    ///
    /// # Returns
    /// Vector of floats representing the embedding (DEFAULT_EMBEDDING_DIM)
    ///
    /// # Errors
    /// - `AppError::EmbeddingFailed` if model inference fails
    /// - `AppError::TokenizationError` if text cannot be tokenized
    ///
    /// # Example
    /// ```rust
    /// use lattice::domain::embedding_constants::DEFAULT_EMBEDDING_DIM;
    /// let embedding = service.embed_single("hello world").await?;
    /// assert_eq!(embedding.len(), DEFAULT_EMBEDDING_DIM);
    /// ```
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for multiple texts in a batch
    ///
    /// More efficient than calling `embed_single` multiple times.
    ///
    /// # Arguments
    /// * `texts` - Slice of text strings to embed
    ///
    /// # Returns
    /// Vector of embeddings in the same order as input texts
    ///
    /// # Example
    /// ```rust
    /// let texts = vec!["hello".to_string(), "world".to_string()];
    /// let embeddings = service.embed_batch(&texts).await?;
    /// assert_eq!(embeddings.len(), 2);
    /// ```
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// Generate embeddings for contextualized chunks
    ///
    /// Specialized method for embedding chunks with context prefixes.
    ///
    /// # Arguments
    /// * `chunks` - Slice of contextualized chunks
    ///
    /// # Returns
    /// Vector of embeddings in the same order as input chunks
    async fn embed_contextualized_chunks(
        &self,
        chunks: &[crate::infrastructure::indexing::chunker::ContextualizedChunk],
    ) -> Result<Vec<Vec<f32>>>;
}
