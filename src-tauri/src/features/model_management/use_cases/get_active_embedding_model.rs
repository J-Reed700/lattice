//! Get Active Embedding Model Use Case
//!
//! Gets the currently active embedding model.

use crate::domain::downloaded_model::DownloadedModel;
use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::shared::error::Result;

/// Use case for getting the active embedding model
pub struct GetActiveEmbeddingModelUseCase {
    repository: DownloadedModelRepository,
}

impl GetActiveEmbeddingModelUseCase {
    pub fn new(repository: DownloadedModelRepository) -> Self {
        Self { repository }
    }

    /// Execute the use case
    ///
    /// # Returns
    ///
    /// Some(DownloadedModel) if an active embedding model exists, None otherwise
    pub async fn execute(&self) -> Result<Option<DownloadedModel>> {
        self.repository.get_active_embedding_model().await
    }
}
