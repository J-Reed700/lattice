//! Get Downloaded Models With Metadata Use Case
//!
//! Returns all downloaded models with metadata.

use crate::domain::downloaded_model::DownloadedModel;
use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::shared::error::Result;

/// Use case for getting all downloaded models with metadata
pub struct GetDownloadedModelsWithMetadataUseCase {
    repository: DownloadedModelRepository,
}

impl GetDownloadedModelsWithMetadataUseCase {
    pub fn new(repository: DownloadedModelRepository) -> Self {
        Self { repository }
    }

    /// Execute the use case
    ///
    /// # Returns
    ///
    /// Vector of all downloaded models, ordered by download date (newest first)
    pub async fn execute(&self) -> Result<Vec<DownloadedModel>> {
        self.repository.list_all().await
    }
}
