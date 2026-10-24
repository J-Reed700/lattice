//! Get Downloaded Models With Metadata Use Case
//!
//! Returns all downloaded models with metadata.

use lattice_core::domain::models::downloaded::DownloadedModel;
use lattice_core::domain::repositories::DownloadedModelRepository;
use lattice_core::shared::error::Result;
use std::sync::Arc;

/// Use case for getting all downloaded models with metadata
pub struct GetDownloadedModelsWithMetadataUseCase {
    repository: Arc<dyn DownloadedModelRepository>,
}

impl GetDownloadedModelsWithMetadataUseCase {
    pub fn new(repository: Arc<dyn DownloadedModelRepository>) -> Self {
        Self { repository }
    }

    /// Execute the use case
    pub async fn execute(&self) -> Result<Vec<DownloadedModel>> {
        self.repository.list().await
    }
}
