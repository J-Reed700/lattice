//! Dynamic embedding adapter implementing [`EmbeddingPort`].
//!
//! Reads the loaded model through a read-only provider. If none is loaded,
//! the degraded adapter returns the existing model-not-installed errors.

use std::sync::Arc;

use async_trait::async_trait;

use crate::application::ports::{EmbeddingPort, MockEmbeddingPort};
use crate::domain::embedding_constants::DEFAULT_EMBEDDING_DIM;

pub struct DynamicEmbedding {
    provider: Arc<dyn crate::application::ports::LoadedEmbeddingModelPort>,
}

impl DynamicEmbedding {
    pub fn new(provider: Arc<dyn crate::application::ports::LoadedEmbeddingModelPort>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl EmbeddingPort for DynamicEmbedding {
    async fn embed_single(&self, text: &str) -> crate::shared::result::Result<Vec<f32>> {
        match self.provider.current_model() {
            Some(inner) => inner.embed_single(text).await,
            None => MockEmbeddingPort::new_degraded().embed_single(text).await,
        }
    }

    async fn embed_query(&self, text: &str) -> crate::shared::result::Result<Vec<f32>> {
        match self.provider.current_model() {
            Some(inner) => inner.embed_query(text).await,
            None => MockEmbeddingPort::new_degraded().embed_single(text).await,
        }
    }

    async fn embed_batch(&self, texts: &[String]) -> crate::shared::result::Result<Vec<Vec<f32>>> {
        match self.provider.current_model() {
            Some(inner) => inner.embed_batch(texts).await,
            None => MockEmbeddingPort::new_degraded().embed_batch(texts).await,
        }
    }

    fn dimension(&self) -> usize {
        self.provider
            .current_model()
            .map(|m| m.dimension())
            .unwrap_or(DEFAULT_EMBEDDING_DIM)
    }

    fn model_identity(&self) -> String {
        self.provider
            .current_model()
            .map(|m| m.model_identity())
            .unwrap_or_else(|| "unavailable".into())
    }

    fn split_text(
        &self,
        text: &str,
        prefix: &str,
    ) -> crate::shared::result::Result<
        Vec<crate::application::ports::embedding_port::EmbeddingTextChunk>,
    > {
        self.provider
            .current_model()
            .ok_or_else(|| {
                crate::shared::error::AppError::AiModelsNotInstalled(
                    "Load an embedding model before indexing".into(),
                )
            })?
            .split_text(text, prefix)
    }

    fn uses_late_chunking(&self) -> bool {
        self.provider
            .current_model()
            .is_some_and(|m| m.uses_late_chunking())
    }

    fn supports_sparse(&self) -> bool {
        self.provider
            .current_model()
            .is_some_and(|m| m.supports_sparse())
    }

    async fn embed_sparse_batch(
        &self,
        texts: &[String],
    ) -> crate::shared::result::Result<Vec<crate::domain::value_objects::SparseEmbedding>> {
        match self.provider.current_model() {
            Some(inner) => inner.embed_sparse_batch(texts).await,
            None => {
                Err(crate::application::ports::embedding_port::sparse_not_supported("unavailable"))
            }
        }
    }

    async fn embed_batch_with_sparse(
        &self,
        texts: &[String],
    ) -> crate::shared::result::Result<(
        Vec<Vec<f32>>,
        Vec<crate::domain::value_objects::SparseEmbedding>,
    )> {
        match self.provider.current_model() {
            Some(inner) => inner.embed_batch_with_sparse(texts).await,
            None => {
                Err(crate::application::ports::embedding_port::sparse_not_supported("unavailable"))
            }
        }
    }

    async fn embed_sparse_query(
        &self,
        text: &str,
    ) -> crate::shared::result::Result<crate::domain::value_objects::SparseEmbedding> {
        match self.provider.current_model() {
            Some(inner) => inner.embed_sparse_query(text).await,
            None => {
                Err(crate::application::ports::embedding_port::sparse_not_supported("unavailable"))
            }
        }
    }

    async fn embed_span_chunks(
        &self,
        span_text: &str,
        chunk_ranges: &[std::ops::Range<usize>],
    ) -> crate::shared::result::Result<Vec<Vec<f32>>> {
        match self.provider.current_model() {
            Some(inner) => inner.embed_span_chunks(span_text, chunk_ranges).await,
            None => {
                let texts = crate::application::ports::embedding_port::span_chunk_texts(
                    span_text,
                    chunk_ranges,
                )?;
                MockEmbeddingPort::new_degraded().embed_batch(&texts).await
            }
        }
    }

    async fn is_ready(&self) -> crate::shared::result::Result<bool> {
        match self.provider.current_model() {
            Some(inner) => inner.is_ready().await,
            None => Ok(false),
        }
    }
}
