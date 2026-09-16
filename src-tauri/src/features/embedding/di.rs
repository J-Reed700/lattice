//! Embedding feature dependency injection.
//!
//! The `Container` owns the embedding runtime handle; the lifecycle calls that
//! read it belong to this feature and are registered from here.

use std::sync::Arc;

use crate::application::ports::EmbeddingPort;
use crate::interfaces::di::Container;
use crate::shared::error::Result;

/// Embedding's registrar surface on `Container`.
impl Container {
    /// Get a ready cached model or load through the embedding lifecycle.
    pub async fn get_or_load_embedding(&self) -> Result<Arc<dyn EmbeddingPort>> {
        self.embedding_runtime
            .get_or_load(|| self.load_embedding_with_fallback())
            .await
    }

    /// Force reload and clear the failure cooldown after active-model changes.
    pub fn invalidate_embedding_cache(&self) {
        self.embedding_runtime.invalidate();
    }

    async fn load_embedding_with_fallback(&self) -> Result<Arc<dyn EmbeddingPort>> {
        crate::infrastructure::embedding_loading::EmbeddingLoader {
            downloaded_models: self.ai.downloaded_model_repo().clone(),
            security: self.core.security_context().clone(),
            expected_dimension: self.search.vector_search().dimension(),
            expected_identity: self.search.embedding_identity().map(str::to_owned),
            strategy: self.search.embedding_strategy(),
        }
        .load()
        .await
    }
}
