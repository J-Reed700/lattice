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

    fn model_identity(&self) -> String {
        "unknown".into()
    }
    fn split_text(
        &self,
        text: &str,
        _prefix: &str,
    ) -> Result<Vec<crate::application::ports::embedding_port::EmbeddingTextChunk>> {
        Ok(if text.is_empty() {
            vec![]
        } else {
            vec![
                crate::application::ports::embedding_port::EmbeddingTextChunk {
                    text: text.into(),
                    start: 0,
                    end: text.len(),
                    token_count: 0,
                },
            ]
        })
    }

    async fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        self.embed_single(text).await
    }

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
        chunks: &[crate::features::indexing::engine::chunker::ContextualizedChunk],
    ) -> Result<Vec<Vec<f32>>>;

    /// True when this service can also produce learned sparse term weights.
    /// Mirrors [`crate::application::ports::EmbeddingPort::supports_sparse`].
    fn supports_sparse(&self) -> bool {
        false
    }

    /// Learned sparse term weights for a batch of passages. Defaults to the
    /// same "not supported" refusal as the port, so existing implementations
    /// (mocks, the remote service) need no changes.
    async fn embed_sparse_batch(
        &self,
        _texts: &[String],
    ) -> Result<Vec<crate::domain::value_objects::SparseEmbedding>> {
        Err(crate::application::ports::embedding_port::sparse_not_supported(&self.model_identity()))
    }

    /// Dense vectors and learned sparse term weights for the same batch.
    /// Mirrors the port; override when one forward pass can produce both.
    async fn embed_batch_with_sparse(
        &self,
        texts: &[String],
    ) -> Result<(
        Vec<Vec<f32>>,
        Vec<crate::domain::value_objects::SparseEmbedding>,
    )> {
        let dense = self.embed_batch(texts).await?;
        let sparse = self.embed_sparse_batch(texts).await?;
        Ok((dense, sparse))
    }

    /// Learned sparse term weights for one retrieval query.
    async fn embed_sparse_query(
        &self,
        text: &str,
    ) -> Result<crate::domain::value_objects::SparseEmbedding> {
        let mut batch = self.embed_sparse_batch(&[text.to_owned()]).await?;
        batch
            .pop()
            .ok_or_else(|| crate::shared::error::AppError::EmbeddingFailed {
                reason: "Sparse forward pass returned no embeddings".into(),
            })
    }

    /// True when chunk vectors come from one forward pass over the whole span
    /// ("late chunking") rather than one pass per chunk.
    fn uses_late_chunking(&self) -> bool {
        false
    }

    /// Embed every chunk of one structure span.
    ///
    /// `chunk_ranges` are UTF-8 byte ranges into `span_text`, in chunk order;
    /// everything before the first range is the shared context prefix. The
    /// default implementation rebuilds the ordinary contextualized chunks and
    /// defers to `embed_contextualized_chunks`, so mocks, the remote service
    /// and any other implementation keep working unchanged.
    async fn embed_span_chunks(
        &self,
        span_text: &str,
        chunk_ranges: &[std::ops::Range<usize>],
    ) -> Result<Vec<Vec<f32>>> {
        use crate::features::indexing::engine::chunker::ContextualizedChunk;
        let texts =
            crate::application::ports::embedding_port::span_chunk_texts(span_text, chunk_ranges)?;
        let prefix = chunk_ranges
            .first()
            .and_then(|range| span_text.get(..range.start))
            .unwrap_or_default();
        let chunks: Vec<ContextualizedChunk> = chunk_ranges
            .iter()
            .zip(texts)
            .enumerate()
            .map(
                |(index, (range, contextualized_content))| ContextualizedChunk {
                    original_content: span_text.get(range.clone()).unwrap_or_default().to_owned(),
                    contextualized_content,
                    context_prefix: prefix.to_owned(),
                    chunk_index: index,
                    token_count: 0,
                    start_idx: range.start,
                    end_idx: range.end,
                },
            )
            .collect();
        self.embed_contextualized_chunks(&chunks).await
    }
}
