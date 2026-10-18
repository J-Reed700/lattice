//! Check Is Downloaded Use Case
//!
//! Checks if a model is already downloaded.

use lattice_core::domain::repositories::DownloadedModelRepository;
use lattice_core::shared::error::Result;
use std::sync::Arc;

/// Use case for checking if a model is downloaded
pub struct CheckIsDownloadedUseCase {
    repository: Arc<dyn DownloadedModelRepository>,
}

impl CheckIsDownloadedUseCase {
    pub fn new(repository: Arc<dyn DownloadedModelRepository>) -> Self {
        Self { repository }
    }

    /// Execute the use case
    ///
    /// # Arguments
    ///
    /// * `model_id` - The model identifier to check
    ///
    /// # Returns
    ///
    /// true if model is downloaded, false otherwise
    pub async fn execute(&self, model_id: &str) -> Result<bool> {
        self.repository.is_downloaded(model_id).await
    }
}
