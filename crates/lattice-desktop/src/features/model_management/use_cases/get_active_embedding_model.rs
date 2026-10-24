//! Get Active Embedding Model Use Case
//!
//! Gets the currently active embedding model.

use lattice_core::application::ports::active_models::ActiveModels;
use lattice_core::domain::models::downloaded::DownloadedModel;
use lattice_core::shared::error::Result;
use std::sync::Arc;

/// Use case for getting the active embedding model
pub struct GetActiveEmbeddingModelUseCase {
    active_models: Arc<dyn ActiveModels>,
}

impl GetActiveEmbeddingModelUseCase {
    pub fn new(active_models: Arc<dyn ActiveModels>) -> Self {
        Self { active_models }
    }

    /// Execute the use case
    ///
    /// # Returns
    ///
    /// Some(DownloadedModel) if an active embedding model exists, None otherwise
    pub async fn execute(&self) -> Result<Option<DownloadedModel>> {
        self.active_models.get_active_embedding_model().await
    }
}
