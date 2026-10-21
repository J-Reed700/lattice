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

    async fn is_ready(&self) -> lattice_core::shared::error::Result<bool> {
        match self.provider.current_model() {
            Some(inner) => inner.is_ready().await,
            None => Ok(false),
        }
    }
}
