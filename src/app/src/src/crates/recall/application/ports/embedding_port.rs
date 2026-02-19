//! Embedding generation port for the application layer.
//!
//! This port defines the interface for embedding generation services.
//! Infrastructure implementations can use ONNX Runtime, remote APIs, or other
//! embedding backends.
//!
//! # Purpose
//!
//! - Abstracts embedding model implementation details
//! - Allows switching between local and remote embedding services
//! - Enables testing with mock implementations
//! - Enforces contract for embedding dimensionality
//!
//! # Infrastructure Implementations
//!
//! - `OnnxEmbeddingAdapter` - Local ONNX Runtime models (DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME, etc.)
//! - `RemoteEmbeddingAdapter` - OpenAI, Anthropic, or other remote embedding APIs
//! - `MockEmbeddingAdapter` - Test implementation returning fixed embeddings
//!
//! # Example Usage
//!
//! ```rust
//! use crate::application::ports::EmbeddingPort;
//!
//! async fn embed_document(
//!     embedder: &impl EmbeddingPort,
//!     document: &str,
//! ) -> Result<Vec<f32>> {
//!     let embedding = embedder.embed_single(document).await?;
//!     assert_eq!(embedding.len(), embedder.dimension());
//!     Ok(embedding)
//! }
//! ```

use crate::shared::result::Result;
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
    /// # Arguments
    ///
    /// * `text` - The text to embed. May be empty or very long.
    ///
    /// # Returns
    ///
    /// A vector of floats representing the embedding. The length must match `dimension()`.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if text is invalid (e.g., too long for model)
    /// - `AppError::Network` if using remote API and network fails
    /// - `AppError::EmbeddingFailed` if model inference fails
    ///
    /// # Example
    ///
    /// ```rust
    /// let embedding = embedder.embed_single("Hello world").await?;
    /// assert_eq!(embedding.len(), embedder.dimension());
    /// ```
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for a batch of text inputs.
    ///
    /// Batch processing is typically more efficient than calling `embed_single`
    /// repeatedly, especially for GPU-based or remote API implementations.
    ///
    /// # Arguments
    ///
    /// * `texts` - A slice of strings to embed
    ///
    /// # Returns
    ///
    /// A vector of embeddings, one per input text. Each embedding has length `dimension()`.
    /// The order matches the input order.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if batch is too large or contains invalid text
    /// - `AppError::Network` if using remote API and network fails
    /// - `AppError::EmbeddingFailed` if model inference fails
    ///
    /// # Example
    ///
    /// ```rust
    /// let texts = vec!["First doc".into(), "Second doc".into()];
    /// let embeddings = embedder.embed_batch(&texts).await?;
    /// assert_eq!(embeddings.len(), 2);
    /// assert_eq!(embeddings[0].len(), embedder.dimension());
    /// ```
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// Get the dimensionality of embeddings produced by this embedder.
    ///
    /// This value is constant for a given model and used to:
    /// - Validate embedding vectors
    /// - Configure vector search indexes
    /// - Ensure compatibility between embedders
    ///
    /// # Returns
    ///
    /// The embedding dimension (e.g., DEFAULT_EMBEDDING_DIM for the default model)
    ///
    /// # Example
    ///
    /// ```rust
    /// let dim = embedder.dimension();
    /// assert!(dim > 0);
    /// ```
    fn dimension(&self) -> usize;

    /// Check if the embedding service is ready and operational.
    ///
    /// This method is used for health checks and system diagnostics.
    ///
    /// # Returns
    ///
    /// - `Ok(true)` if the service is ready to generate embeddings
    /// - `Ok(false)` if the service is not ready (model not loaded, API unavailable, etc.)
    /// - `Err` if the health check itself fails
    ///
    /// # Example
    ///
    /// ```rust
    /// if embedder.is_ready().await? {
    ///     println!("Embedding service is operational");
    /// } else {
    ///     println!("Embedding service is not ready");
    /// }
    /// ```
    async fn is_ready(&self) -> Result<bool>;
}
