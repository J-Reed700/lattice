//! `EmbeddingPort` for the Candle service.

use std::ops::Range;
use std::sync::Arc;

use async_trait::async_trait;

use crate::features::embedding::late_chunking::vector_identity;
use crate::features::embedding::sparse_head::MAX_QUERY_TERMS;
use lattice_core::application::ports::embedding_port::sparse_not_supported;
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::domain::value_objects::SparseEmbedding;
use lattice_core::shared::error::AppError;
use lattice_core::shared::error::Result;

use super::CandleEmbeddingService;

#[async_trait]
impl EmbeddingPort for CandleEmbeddingService {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        if text.is_empty() {
            return Err(AppError::InvalidInput("Cannot embed empty text".into()));
        }
        self.embed_prepared(
            self.prefixes.document_input(text).into_owned(),
            self.prefixes.document.len(),
        )
        .await
    }

    async fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        let query = self.prefixes.query_input(text).into_owned();
        if let Some(cached) = self.cached_query(&query) {
            return Ok(Vec::clone(&cached));
        }
        let vector = Arc::new(
            self.embed_prepared(query.clone(), self.prefixes.query.len())
                .await?,
        );
        self.cache_query(query, Arc::clone(&vector));
        Ok(Vec::clone(&vector))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if self.strategy.is_late_chunking() {
            // One pass per text: mean pooling over each text's own content
            // tokens is what keeps these in the late-chunked space.
            let mut vectors = Vec::with_capacity(texts.len());
            for text in self.as_documents(texts) {
                vectors.push(
                    self.embed_content_mean(&text, self.prefixes.document.len())
                        .await?,
                );
            }
            return Ok(vectors);
        }
        Ok(self
            .forward_batched(self.as_documents(texts), None)
            .await?
            .0)
    }

    fn split_text(
        &self,
        text: &str,
        prefix: &str,
    ) -> Result<Vec<lattice_core::application::ports::embedding_port::EmbeddingTextChunk>> {
        // The model's document prefix is part of every passage the runtime
        // embeds, so it has to be part of what the splitter measures against
        // the window — otherwise a chunk sized to exactly fit overflows by the
        // prefix's tokens at embedding time. It goes outside the chunker's
        // context prefix, matching the order `embed_batch` builds.
        if self.prefixes.document.is_empty() {
            return self.input_policy.split(text, prefix);
        }
        self.input_policy
            .split(text, &format!("{}{prefix}", self.prefixes.document))
    }

    fn window_parts(
        &self,
        text: &str,
    ) -> Result<Vec<lattice_core::application::ports::embedding_port::EmbeddingTextChunk>> {
        if text.is_empty() {
            return Ok(Vec::new());
        }
        let tokens = self
            .input_policy
            .count(&self.prefixes.document_input(text))?;
        if tokens > self.input_policy.max_tokens {
            return <Self as EmbeddingPort>::split_text(self, text, "");
        }
        Ok(vec![
            lattice_core::application::ports::embedding_port::EmbeddingTextChunk {
                text: text.to_owned(),
                start: 0,
                end: text.len(),
                token_count: tokens,
            },
        ])
    }

    fn uses_late_chunking(&self) -> bool {
        self.strategy.is_late_chunking()
    }

    async fn embed_span_chunks(
        &self,
        span_text: &str,
        chunk_ranges: &[Range<usize>],
    ) -> Result<Vec<Vec<f32>>> {
        self.embed_span(span_text, chunk_ranges).await
    }

    fn supports_sparse(&self) -> bool {
        self.sparse_head.is_some()
    }

    async fn embed_sparse_batch(&self, texts: &[String]) -> Result<Vec<SparseEmbedding>> {
        Ok(self.embed_batch_with_sparse(texts).await?.1)
    }

    async fn embed_batch_with_sparse(
        &self,
        texts: &[String],
    ) -> Result<(Vec<Vec<f32>>, Vec<SparseEmbedding>)> {
        CandleEmbeddingService::embed_batch_with_sparse(self, texts).await
    }

    async fn embed_sparse_query(&self, text: &str) -> Result<SparseEmbedding> {
        if self.sparse_head.is_none() {
            return Err(sparse_not_supported(self.identity.as_str()));
        }
        // A query is pruned much harder than a passage: every surviving term
        // becomes a row in the scoring join.
        let (_, mut sparse) = self
            .forward_batched(
                vec![self.prefixes.query_input(text).into_owned()],
                Some(MAX_QUERY_TERMS),
            )
            .await?;
        Ok(sparse.pop().unwrap_or_default())
    }

    fn model_identity(&self) -> String {
        vector_identity(self.identity.as_str(), self.strategy, self.prefixes)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}
