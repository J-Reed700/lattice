//! Dynamic embedding adapter implementing [`EmbeddingPort`].
//!
//! Reads the loaded model through a read-only provider. If none is loaded,
//! the degraded adapter returns the existing model-not-installed errors.

use std::sync::Arc;

use async_trait::async_trait;

use lattice_core::application::ports::{EmbeddingPort, MockEmbeddingPort};
use lattice_core::domain::models::embedding_defaults::DEFAULT_EMBEDDING_DIM;

pub struct DynamicEmbedding {
    provider: Arc<dyn lattice_core::application::ports::LoadedEmbeddingModelPort>,
}

impl DynamicEmbedding {
    pub fn new(
        provider: Arc<dyn lattice_core::application::ports::LoadedEmbeddingModelPort>,
    ) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl EmbeddingPort for DynamicEmbedding {
    async fn embed_single(&self, text: &str) -> lattice_core::shared::error::Result<Vec<f32>> {
        match self.provider.current_model() {
            Some(inner) => inner.embed_single(text).await,
            None => MockEmbeddingPort::new_degraded().embed_single(text).await,
        }
    }

    async fn embed_query(&self, text: &str) -> lattice_core::shared::error::Result<Vec<f32>> {
        match self.provider.current_model() {
            Some(inner) => inner.embed_query(text).await,
            None => MockEmbeddingPort::new_degraded().embed_single(text).await,
        }
    }

    async fn embed_batch(
        &self,
        texts: &[String],
    ) -> lattice_core::shared::error::Result<Vec<Vec<f32>>> {
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

    fn accelerator_bytes(&self) -> u64 {
        self.provider
            .current_model()
            .map_or(0, |m| m.accelerator_bytes())
    }

    fn split_text(
        &self,
        text: &str,
        prefix: &str,
    ) -> lattice_core::shared::error::Result<
        Vec<lattice_core::application::ports::embedding_port::EmbeddingTextChunk>,
    > {
        self.provider
            .current_model()
            .ok_or_else(|| {
                lattice_core::shared::error::AppError::AiModelsNotInstalled(
                    "Load an embedding model before indexing".into(),
                )
            })?
            .split_text(text, prefix)
    }

    fn window_parts(
        &self,
        text: &str,
    ) -> lattice_core::shared::error::Result<
        Vec<lattice_core::application::ports::embedding_port::EmbeddingTextChunk>,
    > {
        self.provider
            .current_model()
            .ok_or_else(|| {
                lattice_core::shared::error::AppError::AiModelsNotInstalled(
                    "Load an embedding model before indexing".into(),
                )
            })?
            .window_parts(text)
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
    ) -> lattice_core::shared::error::Result<
        Vec<lattice_core::domain::value_objects::SparseEmbedding>,
    > {
        match self.provider.current_model() {
            Some(inner) => inner.embed_sparse_batch(texts).await,
            None => Err(
                lattice_core::application::ports::embedding_port::sparse_not_supported(
                    "unavailable",
                ),
            ),
        }
    }

    async fn embed_batch_with_sparse(
        &self,
        texts: &[String],
    ) -> lattice_core::shared::error::Result<(
        Vec<Vec<f32>>,
        Vec<lattice_core::domain::value_objects::SparseEmbedding>,
    )> {
        match self.provider.current_model() {
            Some(inner) => inner.embed_batch_with_sparse(texts).await,
            None => Err(
                lattice_core::application::ports::embedding_port::sparse_not_supported(
                    "unavailable",
                ),
            ),
        }
    }

    async fn embed_sparse_query(
        &self,
        text: &str,
    ) -> lattice_core::shared::error::Result<lattice_core::domain::value_objects::SparseEmbedding>
    {
        match self.provider.current_model() {
            Some(inner) => inner.embed_sparse_query(text).await,
            None => Err(
                lattice_core::application::ports::embedding_port::sparse_not_supported(
                    "unavailable",
                ),
            ),
        }
    }

    async fn embed_span_chunks(
        &self,
        span_text: &str,
        chunk_ranges: &[std::ops::Range<usize>],
    ) -> lattice_core::shared::error::Result<Vec<Vec<f32>>> {
        match self.provider.current_model() {
            Some(inner) => inner.embed_span_chunks(span_text, chunk_ranges).await,
            None => {
                let texts = lattice_core::application::ports::embedding_port::span_chunk_texts(
                    span_text,
                    chunk_ranges,
                )?;
                MockEmbeddingPort::new_degraded().embed_batch(&texts).await
            }
        }
    }

    async fn is_ready(&self) -> lattice_core::shared::error::Result<bool> {
        match self.provider.current_model() {
            Some(inner) => inner.is_ready().await,
            None => Ok(false),
        }
    }
}
