//! Dynamic embedding adapter implementing [`EmbeddingPort`].
//!
//! Reads from the shared `embedding_cache` (populated once a real model
//! finishes downloading). If no real model is loaded, falls back to
//! `MockEmbeddingPort::new_degraded()` so callers still get deterministic
//! (but non-semantic) output.

use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use crate::application::ports::{EmbeddingPort, MockEmbeddingPort};
use crate::domain::embedding_constants::DEFAULT_EMBEDDING_DIM;

pub struct DynamicEmbedding {
    cache: Arc<RwLock<Option<Arc<dyn EmbeddingPort>>>>,
}

impl DynamicEmbedding {
    pub fn new(cache: Arc<RwLock<Option<Arc<dyn EmbeddingPort>>>>) -> Self {
        Self { cache }
    }

    fn cached(&self) -> crate::shared::result::Result<Option<Arc<dyn EmbeddingPort>>> {
        let guard = self.cache.read().map_err(|e| {
            crate::shared::error::AppError::InternalError(format!(
                "Embedding cache lock poisoned: {}",
                e
            ))
        })?;
        Ok(guard.as_ref().cloned())
    }
}

#[async_trait]
impl EmbeddingPort for DynamicEmbedding {
    async fn embed_single(&self, text: &str) -> crate::shared::result::Result<Vec<f32>> {
        match self.cached()? {
            Some(inner) => inner.embed_single(text).await,
            None => MockEmbeddingPort::new_degraded().embed_single(text).await,
        }
    }

    async fn embed_batch(
        &self,
        texts: &[String],
    ) -> crate::shared::result::Result<Vec<Vec<f32>>> {
        match self.cached()? {
            Some(inner) => inner.embed_batch(texts).await,
            None => MockEmbeddingPort::new_degraded().embed_batch(texts).await,
        }
    }

    fn dimension(&self) -> usize {
        DEFAULT_EMBEDDING_DIM
    }

    async fn is_ready(&self) -> crate::shared::result::Result<bool> {
        match self.cached()? {
            Some(inner) => inner.is_ready().await,
            None => Ok(false),
        }
    }
}
