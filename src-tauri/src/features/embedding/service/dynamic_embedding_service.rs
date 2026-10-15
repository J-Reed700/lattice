//! Dynamic embedding service adapter.
//!
//! Uses a read-only model provider and exposes EmbeddingServiceTrait.

use crate::features::embedding::EmbeddingServiceTrait;
use async_trait::async_trait;
use lattice_core::application::ports::{EmbeddingPort, MockEmbeddingPort};
use lattice_core::shared::error::Result;
use std::sync::Arc;

#[derive(Clone)]
pub struct DynamicEmbeddingService {
    provider: Arc<dyn lattice_core::application::ports::LoadedEmbeddingModelPort>,
}

impl DynamicEmbeddingService {
    pub fn new(
        provider: Arc<dyn lattice_core::application::ports::LoadedEmbeddingModelPort>,
    ) -> Self {
        Self { provider }
    }

    fn get_embedding_port(&self) -> Arc<dyn EmbeddingPort> {
        self.provider
            .current_model()
            .unwrap_or_else(|| Arc::new(MockEmbeddingPort::new_degraded()))
    }
}

#[async_trait]
impl EmbeddingServiceTrait for DynamicEmbeddingService {
    fn model_identity(&self) -> String {
        self.get_embedding_port().model_identity()
    }
    fn split_text(
        &self,
        text: &str,
        prefix: &str,
    ) -> Result<Vec<lattice_core::application::ports::embedding_port::EmbeddingTextChunk>> {
        self.get_embedding_port().split_text(text, prefix)
    }

    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        let service = self.get_embedding_port();
        service.embed_single(text).await
    }

    async fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        self.get_embedding_port().embed_query(text).await
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let service = self.get_embedding_port();
        service.embed_batch(texts).await
    }

    async fn embed_contextualized_chunks(
        &self,
        chunks: &[lattice_core::domain::value_objects::ContextualizedChunk],
    ) -> Result<Vec<Vec<f32>>> {
        let texts: Vec<String> = chunks
            .iter()
            .map(|chunk| chunk.contextualized_content.clone())
            .collect();
        let service = self.get_embedding_port();
        service.embed_batch(&texts).await
    }

    fn uses_late_chunking(&self) -> bool {
        self.get_embedding_port().uses_late_chunking()
    }

    fn supports_sparse(&self) -> bool {
        self.get_embedding_port().supports_sparse()
    }

    async fn embed_sparse_batch(
        &self,
        texts: &[String],
    ) -> Result<Vec<lattice_core::domain::value_objects::SparseEmbedding>> {
        self.get_embedding_port().embed_sparse_batch(texts).await
    }

    async fn embed_batch_with_sparse(
        &self,
        texts: &[String],
    ) -> Result<(
        Vec<Vec<f32>>,
        Vec<lattice_core::domain::value_objects::SparseEmbedding>,
    )> {
        self.get_embedding_port()
            .embed_batch_with_sparse(texts)
            .await
    }

    async fn embed_sparse_query(
        &self,
        text: &str,
    ) -> Result<lattice_core::domain::value_objects::SparseEmbedding> {
        self.get_embedding_port().embed_sparse_query(text).await
    }

    async fn embed_span_chunks(
        &self,
        span_text: &str,
        chunk_ranges: &[std::ops::Range<usize>],
    ) -> Result<Vec<Vec<f32>>> {
        self.get_embedding_port()
            .embed_span_chunks(span_text, chunk_ranges)
            .await
    }
}
