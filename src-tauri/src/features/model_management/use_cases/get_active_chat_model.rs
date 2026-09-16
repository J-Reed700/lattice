//! Get Active Chat Model Use Case
//!
//! Gets the currently active chat model.

use crate::domain::downloaded_model::DownloadedModel;
use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::shared::error::Result;

/// Use case for getting the active chat model
pub struct GetActiveChatModelUseCase {
    repository: DownloadedModelRepository,
}

impl GetActiveChatModelUseCase {
    pub fn new(repository: DownloadedModelRepository) -> Self {
        Self { repository }
    }

    /// Execute the use case
    ///
    /// # Returns
    ///
    /// Some(DownloadedModel) if an active chat model exists, None otherwise
    pub async fn execute(&self) -> Result<Option<DownloadedModel>> {
        self.repository.get_active_chat_model().await
    }
}
