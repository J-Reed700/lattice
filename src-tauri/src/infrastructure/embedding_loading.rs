//! Loads embedding artifacts and validates compatibility before publication.

use crate::application::ports::EmbeddingPort;
use crate::features::embedding::candle_service::CandleEmbeddingService;
use crate::features::embedding::late_chunking::EmbeddingStrategy;
use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::infrastructure::security::SecurityContext;
use crate::shared::error::{AppError, Result};
use std::sync::Arc;

pub(crate) struct EmbeddingLoader {
    pub downloaded_models: Arc<DownloadedModelRepository>,
    pub security: Arc<SecurityContext>,
    pub expected_dimension: usize,
    pub expected_identity: Option<String>,
    /// How chunk vectors are produced, straight from the user's settings.
    ///
    /// The strategy is part of the vector space, not a detail of inference:
    /// the service reports a different `model_identity()` for each, and the
    /// identity check below is what stops a model loaded under one strategy
    /// from writing into an index keyed on the other.
    pub strategy: EmbeddingStrategy,
}

impl EmbeddingLoader {
    /// Try to load the active embedding model
    ///
    /// Returns Ok(Some(embedding)) if active model loaded successfully
    /// Returns Ok(None) if no active model or file doesn't exist
    /// Returns Err only on unrecoverable errors
    async fn try_load_active_embedding_model(&self) -> Result<Option<Arc<dyn EmbeddingPort>>> {
        let active_model = match self.downloaded_models.get_active_embedding_model().await {
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
                return Ok(None);
            }
        };

        tracing::info!(
            "Found active embedding model in database: {} ({})",
            active_model.model_name(),
            active_model.model_id()
        );

        let model_path = match active_model.loadable_path() {
            Some(p) => p.to_path_buf(),
            None => return Ok(None),
        };

        if !model_path.exists() {
            tracing::warn!(
                "Active embedding model artifact not found: {} - user may have deleted it manually",
                model_path.display()
            );
            return Ok(None);
        }

        let models_dir = model_path
            .parent()
            .ok_or_else(|| AppError::Security("Model path has no parent directory".into()))?
            .to_path_buf();

        let _validated_path = match self
            .security
            .input_validator()
            .validate_model_path(&model_path, &models_dir)
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
                return Ok(None);
            }
        };

        let model_dir = active_model.location().enclosing_dir().ok_or_else(|| {
            AppError::ModelLoadFailed(format!(
                "Active embedding model {} has no enclosing directory (remote-hosted?)",
                active_model.model_name()
            ))
        })?;

        tracing::info!(
            "Loading Candle embedding model from validated dir: {}",
            model_dir.display()
        );

        match CandleEmbeddingService::new(&model_dir).map(|s| s.with_strategy(self.strategy)) {
            Ok(service) => {
                if self.expected_identity.as_deref() != Some(service.model_identity().as_str()) {
                    return Err(AppError::ModelLoadFailed("Embedding model changed. Restart Lattice to open its prepared search generation.".into()));
                }
                let expected_dim = self.expected_dimension;
                let actual_dim = service.dimension();
                if actual_dim != expected_dim {
                    tracing::error!(
                        "Embedding dimension mismatch: model '{}' produces {}-dim vectors \
                         but the vector index expects {}-dim. Restart the app to rebuild \
                         the index at the new dimension.",
                        active_model.model_name(),
                        actual_dim,
                        expected_dim
                    );
                    return Err(AppError::ModelLoadFailed(format!(
                        "Model '{}' produces {}-dimensional embeddings but the search index \
                         requires {}. Restart the app to migrate the index.",
                        active_model.model_name(),
                        actual_dim,
                        expected_dim
                    )));
                }

                tracing::info!(
                    "Successfully loaded active embedding model: {} ({}-dim, {:?})",
                    active_model.model_name(),
                    actual_dim,
                    service.architecture()
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

    pub(crate) async fn load(&self) -> Result<Arc<dyn EmbeddingPort>> {
        use crate::application::ports::MockEmbeddingPort;

        tracing::debug!("Step 1: Checking for active downloaded embedding model...");
        match self.try_load_active_embedding_model().await {
            Ok(Some(embedding)) => {
                tracing::info!("Using active downloaded embedding model");
                return Ok(embedding);
            }
            Ok(None) => {
                tracing::debug!("No active embedding model available, using mock...");
            }
            Err(e @ AppError::ModelLoadFailed(_)) => {
                return Err(e);
            }
            Err(e) => {
                tracing::warn!("Error loading active embedding model: {}, using mock...", e);
            }
        }

        tracing::info!("Using Mock embedding implementation");
        tracing::info!("💡 Download an embedding model to enable semantic search features");
        Ok(Arc::new(MockEmbeddingPort::new_degraded()) as Arc<dyn EmbeddingPort>)
    }
}
