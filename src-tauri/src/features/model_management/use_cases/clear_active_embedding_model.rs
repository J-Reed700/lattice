//! Clear Active Embedding Model Use Case
//!
//! Deactivates the currently active embedding model.

use lattice_core::application::ports::active_models::ActiveModels;
use lattice_core::shared::error::Result;
use std::sync::Arc;
use tracing::info;

/// Use case for clearing the active embedding model
pub struct ClearActiveEmbeddingModelUseCase {
    active_models: Arc<dyn ActiveModels>,
}

impl ClearActiveEmbeddingModelUseCase {
    pub fn new(active_models: Arc<dyn ActiveModels>) -> Self {
        Self { active_models }
    }

    /// Execute the use case
    ///
    /// Sets is_active_for_embedding=0 for all models
    pub async fn execute(&self) -> Result<()> {
        self.active_models.clear_active_embedding_model().await?;
        info!("Active embedding model cleared");
        Ok(())
    }
}
