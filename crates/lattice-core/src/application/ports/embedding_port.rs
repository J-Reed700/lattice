//! Embedding generation port for the application layer.
//!
//! This port defines the interface for embedding generation services.
//! The app's implementation is a model served by the bundled llama-server
//! (`features::embedding::llama_cpp`); tests implement it with fakes.

use crate::shared::error::Result;
use async_trait::async_trait;

/// Port for embedding generation services.
///
/// Implementations must:
/// - Generate consistent embedding dimensions
/// - Handle empty or very long text gracefully
/// - Support both single and batch embedding generation
/// - Be thread-safe (`Send + Sync`)
#[async_trait]
pub trait EmbeddingPort: Send + Sync {
    /// Generate an embedding for a single text input.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if text is invalid (e.g., too long for model)
    /// - `AppError::Network` if using remote API and network fails
    /// - `AppError::EmbeddingFailed` if model inference fails
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>>;

    /// Embed a retrieval query, including model-specific task instructions.
    async fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        self.embed_single(text).await
    }

    /// Generate embeddings for a batch of text inputs.
    ///
    /// Batch processing is typically more efficient than calling `embed_single`
    /// repeatedly, especially for GPU-based or remote API implementations.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if batch is too large or contains invalid text
    /// - `AppError::Network` if using remote API and network fails
    /// - `AppError::EmbeddingFailed` if model inference fails
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// Split a passage with the same input policy used for inference. Offsets are UTF-8 bytes.
    /// Implementations without a tokenizer must reject unsupported lengths at inference.
    fn split_text(&self, text: &str, _prefix: &str) -> Result<Vec<EmbeddingTextChunk>> {
        Ok(if text.is_empty() {
            vec![]
        } else {
            vec![EmbeddingTextChunk {
                text: text.to_owned(),
                start: 0,
                end: text.len(),
                token_count: 0,
            }]
        })
    }

    /// Split `text` only where the model window cannot take it in one pass.
    ///
    /// Re-embedding a stored chunk uses this rather than [`Self::split_text`]:
    /// the stored text already carries the context prefix it was sized with,
    /// so the chunk-size split would cut nearly every full chunk in two.
    fn window_parts(&self, text: &str) -> Result<Vec<EmbeddingTextChunk>> {
        self.split_text(text, "")
    }

    /// Identity of the vector space, including model artifacts and preprocessing version.
    fn model_identity(&self) -> String {
        "unknown".to_owned()
    }

    /// Get the dimensionality of embeddings produced by this embedder.
    ///
    /// This value is constant for a given model and used to:
    /// - Validate embedding vectors
    /// - Configure vector search indexes
    /// - Ensure compatibility between embedders
    fn dimension(&self) -> usize;

    /// Check if the embedding service is ready and operational.
    ///
    /// This method is used for health checks and system diagnostics.
    async fn is_ready(&self) -> Result<bool>;
}

/// A lossless slice of a source passage that fits the embedder's input budget.
#[derive(Debug, Clone)]
pub struct EmbeddingTextChunk {
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub token_count: usize,
}
