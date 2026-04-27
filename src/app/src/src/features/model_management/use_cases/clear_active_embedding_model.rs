//! Clear Active Embedding Model Use Case
//!
//! Deactivates the currently active embedding model.

use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::shared::error::Result;
use tracing::info;

/// Use case for clearing the active embedding model
pub struct ClearActiveEmbeddingModelUseCase {
    repository: DownloadedModelRepository,
}

impl ClearActiveEmbeddingModelUseCase {
    pub fn new(repository: DownloadedModelRepository) -> Self {
        Self { repository }
    }

    /// Execute the use case
    ///
    /// Sets is_active_for_embedding=0 for all models
    pub async fn execute(&self) -> Result<()> {
        self.repository.clear_active_embedding_model().await?;
        info!("Active embedding model cleared");
        Ok(())
    }
}
