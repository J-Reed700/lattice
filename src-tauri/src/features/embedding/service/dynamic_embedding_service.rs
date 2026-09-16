//! Dynamic embedding service adapter.
//!
//! Uses a read-only model provider and exposes EmbeddingServiceTrait.

use crate::application::ports::{EmbeddingPort, MockEmbeddingPort};
use crate::features::embedding::EmbeddingServiceTrait;
use crate::shared::error::Result;
use async_trait::async_trait;
use std::sync::Arc;

#[derive(Clone)]
pub struct DynamicEmbeddingService {
    provider: Arc<dyn crate::application::ports::LoadedEmbeddingModelPort>,
}

impl DynamicEmbeddingService {
    pub fn new(provider: Arc<dyn crate::application::ports::LoadedEmbeddingModelPort>) -> Self {
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
    ) -> Result<Vec<crate::application::ports::embedding_port::EmbeddingTextChunk>> {
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
        chunks: &[crate::features::indexing::engine::chunker::ContextualizedChunk],
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
    ) -> Result<Vec<crate::domain::value_objects::SparseEmbedding>> {
        self.get_embedding_port().embed_sparse_batch(texts).await
    }

    async fn embed_batch_with_sparse(
        &self,
        texts: &[String],
    ) -> Result<(
        Vec<Vec<f32>>,
        Vec<crate::domain::value_objects::SparseEmbedding>,
    )> {
        self.get_embedding_port()
            .embed_batch_with_sparse(texts)
            .await
    }

    async fn embed_sparse_query(
        &self,
        text: &str,
    ) -> Result<crate::domain::value_objects::SparseEmbedding> {
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
