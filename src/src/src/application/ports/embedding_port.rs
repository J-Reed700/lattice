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

use crate::domain::value_objects::SparseEmbedding;
use crate::shared::error::AppError;
use crate::shared::result::Result;
use async_trait::async_trait;
use std::ops::Range;

/// The error every embedder returns from the sparse methods it does not
/// implement. Callers treat it as "this model has no sparse head", never as a
/// retrieval failure, so a sparse branch simply drops out of the fusion.
pub fn sparse_not_supported(model_identity: &str) -> AppError {
    AppError::ServiceNotAvailable(format!(
        "Learned sparse retrieval is not supported by the loaded embedding model ({model_identity})"
    ))
}

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

    /// Embed a retrieval query, including model-specific task instructions.
    async fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        self.embed_single(text).await
    }

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

    /// True when this embedder derives chunk vectors from a single forward pass
    /// over the whole span ("late chunking") instead of embedding each chunk on
    /// its own. Callers that can group chunks per structure span should check
    /// this and route those spans through [`EmbeddingPort::embed_span_chunks`].
    fn uses_late_chunking(&self) -> bool {
        false
    }

    /// Embed every chunk of one structure span.
    ///
    /// `chunk_ranges` are UTF-8 byte ranges into `span_text`, in chunk order.
    /// Anything before the first range is the shared context prefix: with late
    /// chunking it conditions the pass once, and on the chunk-first path it is
    /// prepended to each chunk. Both orders therefore see the same text.
    ///
    /// The default implementation is the chunk-first path, so implementations
    /// without a local tokenizer need no changes.
    async fn embed_span_chunks(
        &self,
        span_text: &str,
        chunk_ranges: &[Range<usize>],
    ) -> Result<Vec<Vec<f32>>> {
        let texts = span_chunk_texts(span_text, chunk_ranges)?;
        self.embed_batch(&texts).await
    }

    /// True when this embedder can also produce learned sparse term weights
    /// (a BGE-M3 style sparse head) alongside its dense vector.
    ///
    /// Default `false`: remote embedders, mocks, and every model without a
    /// sparse head keep working unchanged, and the sparse retrieval branch is
    /// simply never started for them.
    fn supports_sparse(&self) -> bool {
        false
    }

    /// Learned sparse term weights for a batch of passages.
    ///
    /// The default implementation refuses, because producing a term-weight
    /// vector requires a model head that most embedders do not have. Check
    /// [`EmbeddingPort::supports_sparse`] first; callers that do not are
    /// handed an error they must not treat as a retrieval failure.
    async fn embed_sparse_batch(&self, _texts: &[String]) -> Result<Vec<SparseEmbedding>> {
        Err(sparse_not_supported(&self.model_identity()))
    }

    /// Dense vectors and learned sparse term weights for the same batch.
    ///
    /// An embedder that computes both from one set of hidden states should
    /// override this; the default runs the two batches separately, which is
    /// correct but pays for the encoder twice. Indexing calls this rather than
    /// the two methods so the saving is available wherever it exists.
    async fn embed_batch_with_sparse(
        &self,
        texts: &[String],
    ) -> Result<(Vec<Vec<f32>>, Vec<SparseEmbedding>)> {
        let dense = self.embed_batch(texts).await?;
        let sparse = self.embed_sparse_batch(texts).await?;
        Ok((dense, sparse))
    }

    /// Learned sparse term weights for one retrieval query.
    ///
    /// Queries and passages go through the same head — BGE-M3 does not use a
    /// query-side instruction for its sparse output — so the default simply
    /// runs the batch path with one element.
    async fn embed_sparse_query(&self, text: &str) -> Result<SparseEmbedding> {
        let mut batch = self.embed_sparse_batch(&[text.to_owned()]).await?;
        batch.pop().ok_or_else(|| AppError::EmbeddingFailed {
            reason: "Sparse forward pass returned no embeddings".into(),
        })
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

/// The chunk-first inputs for a span: the span's context prefix (everything
/// before the first chunk range) followed by each chunk's own text. This is the
/// exact text the per-chunk path embeds, which keeps the late-chunking fallback
/// byte-identical to not having used late chunking at all.
pub fn span_chunk_texts(span_text: &str, chunk_ranges: &[Range<usize>]) -> Result<Vec<String>> {
    let Some(first) = chunk_ranges.first() else {
        return Ok(Vec::new());
    };
    let prefix = span_text
        .get(..first.start)
        .ok_or_else(|| AppError::InvalidState("Invalid embedding span prefix boundary".into()))?;
    chunk_ranges
        .iter()
        .map(|range| {
            span_text
                .get(range.clone())
                .map(|text| format!("{prefix}{text}"))
                .ok_or_else(|| {
                    AppError::InvalidState("Invalid embedding span chunk boundary".into())
                })
        })
        .collect()
}

/// A lossless slice of a source passage that fits the embedder's input budget.
#[derive(Debug, Clone)]
pub struct EmbeddingTextChunk {
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub token_count: usize,
}

#[cfg(test)]
mod sparse_default_tests {
    use super::*;

    /// The shape every embedder without a sparse head has: it implements only
    /// the two required methods and inherits every sparse default.
    struct DenseOnly;

    #[async_trait]
    impl EmbeddingPort for DenseOnly {
        async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.25; 3])
        }
        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.25; 3]).collect())
        }
        fn dimension(&self) -> usize {
            3
        }
        fn model_identity(&self) -> String {
            "sha256:dense-only".to_owned()
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    #[test]
    fn a_dense_only_embedder_does_not_claim_sparse_support() {
        assert!(!DenseOnly.supports_sparse());
    }

    #[tokio::test]
    async fn every_sparse_entry_point_refuses_with_the_same_error() {
        let texts = vec!["anything".to_owned()];
        // Naming the model in the error is what makes a log line actionable:
        // "which model was loaded when sparse retrieval went missing".
        for message in [
            DenseOnly
                .embed_sparse_batch(&texts)
                .await
                .err()
                .map(|e| e.to_string()),
            DenseOnly
                .embed_sparse_query("anything")
                .await
                .err()
                .map(|e| e.to_string()),
            DenseOnly
                .embed_batch_with_sparse(&texts)
                .await
                .err()
                .map(|e| e.to_string()),
        ] {
            let message = message.unwrap_or_default();
            assert!(
                message.contains("not supported") && message.contains("sha256:dense-only"),
                "unexpected refusal: {message}"
            );
        }
    }

    #[tokio::test]
    async fn refusing_sparse_never_costs_the_dense_vector() {
        // The dense path is untouched by the sparse defaults: a caller that
        // checks `supports_sparse()` first still gets normal embeddings.
        let vector = DenseOnly.embed_single("anything").await.unwrap_or_default();
        assert_eq!(vector.len(), DenseOnly.dimension());
    }
}
