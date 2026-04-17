//! Dynamic embedding service adapter.
//!
//! Wraps the embedding cache (EmbeddingPort) and exposes EmbeddingServiceTrait.

use crate::application::ports::{EmbeddingPort, MockEmbeddingPort};
use crate::infrastructure::services::traits::EmbeddingServiceTrait;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct DynamicEmbeddingService {
    cache: Arc<RwLock<Option<Arc<dyn EmbeddingPort>>>>,
}

impl DynamicEmbeddingService {
    pub fn new(cache: Arc<RwLock<Option<Arc<dyn EmbeddingPort>>>>) -> Self {
        Self { cache }
    }

    fn get_embedding_port(&self) -> Result<Arc<dyn EmbeddingPort>> {
        let cache_read = self.cache.read().map_err(|e| {
            AppError::InternalError(format!("Embedding cache lock poisoned: {}", e))
        })?;

        if let Some(service) = cache_read.as_ref() {
            Ok(Arc::clone(service))
        } else {
            Ok(Arc::new(MockEmbeddingPort::new_degraded()))
        }
    }
}

#[async_trait]
impl EmbeddingServiceTrait for DynamicEmbeddingService {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        let service = self.get_embedding_port()?;
        service.embed_single(text).await
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let service = self.get_embedding_port()?;
        service.embed_batch(texts).await
    }

    async fn embed_contextualized_chunks(
        &self,
        chunks: &[crate::infrastructure::indexing::chunker::ContextualizedChunk],
    ) -> Result<Vec<Vec<f32>>> {
        let texts: Vec<String> = chunks
            .iter()
            .map(|chunk| chunk.contextualized_content.clone())
            .collect();
        let service = self.get_embedding_port()?;
        service.embed_batch(&texts).await
    }
}
