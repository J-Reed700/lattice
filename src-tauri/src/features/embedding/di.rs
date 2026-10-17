//! Embedding feature dependency injection.
//!
//! The `Container` owns the embedding runtime handle; the lifecycle calls that
//! read it belong to this feature and are registered from here.

use crate::features::embedding::candle_service::CandleEmbeddingService;
use crate::features::embedding::loading::EmbeddingLoader;
use crate::features::embedding::runtime::ActiveEmbedding;
use std::path::PathBuf;
use std::sync::Arc;

use crate::interfaces::di::Container;
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::domain::value_objects::ArtifactIdentity;
use lattice_core::shared::error::{AppError, Result};

/// Embedding's registrar surface on `Container`.
impl Container {
    /// Get a ready cached model or load through the embedding lifecycle.
    pub async fn get_or_load_embedding(&self) -> Result<Arc<dyn EmbeddingPort>> {
        self.active_embedding().get_or_load().await
    }

    /// Force reload and clear the failure cooldown after active-model changes.
    pub fn invalidate_embedding_cache(&self) {
        self.embedding_runtime.invalidate();
    }

    /// Open the embedding model in `dir` under `identity`, off the async
    /// runtime, so activation can prepare the vector store against it before
    /// the model becomes active.
    pub(crate) async fn open_embedding_for_activation(
        &self,
        dir: PathBuf,
        identity: ArtifactIdentity,
    ) -> Result<Box<dyn EmbeddingPort>> {
        let model =
            tokio::task::spawn_blocking(move || CandleEmbeddingService::open(&dir, identity))
                .await
                .map_err(|e| {
                    AppError::InternalError(format!("Embedding model load task failed: {}", e))
                })??;
        Ok(Box::new(model))
    }

    /// The shared embedding runtime and the loader it falls back to, as one
    /// value: what `get_or_load_embedding` uses, for a holder that must not
    /// keep the container itself.
    pub(crate) fn active_embedding(&self) -> ActiveEmbedding {
        ActiveEmbedding::new(
            Arc::clone(&self.embedding_runtime),
            EmbeddingLoader {
                downloaded_models: self.ai.downloaded_model_repo().clone(),
                security: self.core.security_context().clone(),
                index: self.search.runtime_index(),
                strategy: self.search.embedding_strategy(),
            },
        )
    }
}
