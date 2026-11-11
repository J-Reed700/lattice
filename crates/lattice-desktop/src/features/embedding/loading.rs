//! Loads embedding artifacts and validates compatibility before publication.

use crate::features::embedding::llama_cpp::{EmbeddingServerStarter, LlamaEmbeddingService};
use crate::infrastructure::security::SecurityContext;
use lattice_core::application::ports::active_models::ActiveModels;
use lattice_core::application::ports::{
    EmbeddingIndexBinding, EmbeddingPort, UnavailableEmbedding,
};
use lattice_core::shared::error::{AppError, Result};
use std::sync::Arc;

pub(crate) struct EmbeddingLoader {
    pub active_models: Arc<dyn ActiveModels>,
    pub security: Arc<SecurityContext>,
    pub index: Arc<dyn EmbeddingIndexBinding>,
    /// Starts the llama-server the model runs on.
    pub starter: Arc<dyn EmbeddingServerStarter>,
}

impl EmbeddingLoader {
    /// Try to load the active embedding model
    ///
    /// Returns Ok(Some(embedding)) if active model loaded successfully
    /// Returns Ok(None) only when no embedding model is active
    /// Returns Err when an active model cannot be used
    async fn try_load_active_embedding_model(&self) -> Result<Option<Arc<dyn EmbeddingPort>>> {
        let active_model = match self.active_models.get_active_embedding_model().await {
            Ok(Some(model)) => model,
            Ok(None) => {
                tracing::debug!("No active embedding model configured in database");
                return Ok(None);
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to query active embedding model from database: {}",
                    e
                );
                return Err(e);
            }
        };

        tracing::info!(
            "Found active embedding model in database: {} ({})",
            active_model.model_name(),
            active_model.model_id()
        );

        let model_path = match active_model.loadable_path() {
            Some(p) => p.to_path_buf(),
            None => {
                return Err(AppError::InvalidState(format!(
                    "The active embedding model '{}' has no file on this machine",
                    active_model.model_name()
                )))
            }
        };

        if !model_path.exists() {
            tracing::warn!(
                "Active embedding model artifact not found: {} - user may have deleted it manually",
                model_path.display()
            );
            return Err(AppError::NotFound(format!(
                "The file of the active embedding model '{}' is missing: {}",
                active_model.model_name(),
                model_path.display()
            )));
        }

        let _validated_path = match self
            .security
            .input_validator()
            .validate_model_artifact(&model_path)
        {
            Ok(path) => path,
            Err(e) => {
                tracing::error!(
                    "Path validation failed for model {}: {}",
                    active_model.model_name(),
                    e
                );
                tracing::error!(
                    "Rejecting model load due to security violation (CWE-22: Path Traversal)"
                );
                return Err(AppError::Security(format!(
                    "The active embedding model '{}' failed path validation: {}",
                    active_model.model_name(),
                    e
                )));
            }
        };

        let model_dir = active_model.location().enclosing_dir().ok_or_else(|| {
            AppError::ModelLoadFailed(format!(
                "Active embedding model {} has no enclosing directory (remote-hosted?)",
                active_model.model_name()
            ))
        })?;

        // Activation records the identity; a local active model without one
        // means the row was written outside the repository's guard.
        let Some(identity) = active_model.embedding_artifact_identity().cloned() else {
            tracing::error!(
                model_id = %active_model.model_id(),
                "Active embedding model has no recorded artifact identity"
            );
            return Err(AppError::ModelLoadFailed(
                "Embedding model has no recorded identity. Activate it again in Settings.".into(),
            ));
        };

        tracing::info!(
            "Starting the embedding server for validated model: {}",
            model_path.display()
        );

        let loaded =
            LlamaEmbeddingService::start(Arc::clone(&self.starter), &active_model, identity).await;

        match loaded {
            Ok(service) => {
                self.index.bind_first(&service).await.map_err(|e| {
                    tracing::error!(error = %e, "Failed to initialize embedding search index");
                    AppError::ModelLoadFailed(e.to_string())
                })?;
                let actual_dim = service.dimension();

                tracing::info!(
                    "Successfully loaded active embedding model: {} ({}-dim, {})",
                    active_model.model_name(),
                    actual_dim,
                    service.facts().architecture
                );
                Ok(Some(Arc::new(service) as Arc<dyn EmbeddingPort>))
            }
            Err(e) => {
                tracing::error!(
                    "Failed to load active embedding model {}: {}",
                    active_model.model_name(),
                    e
                );
                Err(AppError::ModelLoadFailed(format!(
                    "Failed to load active embedding model '{}' from '{}': {}",
                    active_model.model_name(),
                    model_dir.display(),
                    e
                )))
            }
        }
    }

    /// The active embedding model, or the port that says why there is none:
    /// not installed, or the error that kept the active model from loading.
    pub(crate) async fn load(&self) -> Arc<dyn EmbeddingPort> {
        tracing::debug!("Step 1: Checking for active downloaded embedding model...");
        match self.try_load_active_embedding_model().await {
            Ok(Some(embedding)) => {
                tracing::info!("Using active downloaded embedding model");
                embedding
            }
            Ok(None) => {
                tracing::info!("No active embedding model; embedding is unavailable");
                tracing::info!("💡 Download an embedding model to enable semantic search features");
                Arc::new(UnavailableEmbedding::not_installed())
            }
            Err(e) => {
                tracing::warn!("Error loading active embedding model: {}", e);
                Arc::new(UnavailableEmbedding::failed_to_load(e))
            }
        }
    }
}

#[cfg(test)]
mod tests;
