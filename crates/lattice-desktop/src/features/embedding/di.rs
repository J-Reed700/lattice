//! Embedding feature dependency injection.
//!
//! The `Container` owns the embedding runtime handle; the lifecycle calls that
//! read it belong to this feature and are registered from here, with
//! [`AppEmbedders`], the embedder every passage index on disk builds with.

use crate::features::embedding::llama_cpp::LlamaEmbeddingService;
use crate::features::embedding::loading::EmbeddingLoader;
use crate::features::embedding::runtime::ActiveEmbedding;
use std::sync::Arc;
use tauri::{AppHandle, Manager, Runtime};

use crate::infrastructure::passage_index::EmbedderSource;
use crate::interfaces::di::Container;
use async_trait::async_trait;
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::domain::models::downloaded::DownloadedModel;
use lattice_core::domain::value_objects::ArtifactIdentity;
use lattice_core::shared::error::Result;

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

    /// Start a server for `model` under `identity`, so activation can
    /// prepare the vector store against it before the model becomes active.
    pub(crate) async fn open_embedding_for_activation(
        &self,
        model: &DownloadedModel,
        identity: ArtifactIdentity,
    ) -> Result<Box<dyn EmbeddingPort>> {
        let service =
            LlamaEmbeddingService::start(Arc::clone(&self.embedding_starter), model, identity)
                .await?;
        Ok(Box::new(service))
    }

    /// The shared embedding runtime and the loader it falls back to, as one
    /// value: what `get_or_load_embedding` uses, for a holder that must not
    /// keep the container itself.
    pub(crate) fn active_embedding(&self) -> ActiveEmbedding {
        ActiveEmbedding::new(
            Arc::clone(&self.embedding_runtime),
            EmbeddingLoader {
                active_models: self.ai.active_models(),
                security: self.core.security_context().clone(),
                index: self.search.runtime_index(),
                starter: Arc::clone(&self.embedding_starter),
            },
        )
    }
}

/// The embedder a passage index builds and searches with: the container's,
/// so the model is loaded once and shared with the chat path. It lives with
/// the embedding feature so any feature that keeps an index takes the same
/// `Arc<dyn EmbedderSource>` from the composition root, without depending on
/// another feature.
pub(crate) struct AppEmbedders<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> AppEmbedders<R> {
    pub(crate) fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

#[async_trait]
impl<R: Runtime> EmbedderSource for AppEmbedders<R> {
    async fn embedder(&self) -> Option<Arc<dyn EmbeddingPort>> {
        let container = self.app.try_state::<Container>()?;
        // No identity means no active embedding model: nothing to load.
        container.search.embedding_identity()?;
        container
            .get_or_load_embedding()
            .await
            .map_err(|error| tracing::warn!(%error, "A passage index has no embedder"))
            .ok()
    }

    /// The identity the library's index is bound to, which is the loaded
    /// model's `model_identity`: read without loading anything.
    async fn identity(&self) -> Option<String> {
        let container = self.app.try_state::<Container>()?;
        container.search.embedding_identity().map(str::to_owned)
    }
}
