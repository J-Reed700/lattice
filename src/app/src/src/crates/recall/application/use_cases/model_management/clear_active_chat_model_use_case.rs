//! Clear Active Chat Model Use Case
//!
//! Deactivates the currently active chat model.

use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::shared::error::Result;
use tracing::info;

/// Use case for clearing the active chat model
pub struct ClearActiveChatModelUseCase {
    repository: DownloadedModelRepository,
}

impl ClearActiveChatModelUseCase {
    pub fn new(repository: DownloadedModelRepository) -> Self {
        Self { repository }
    }

    /// Execute the use case
    ///
    /// Sets is_active_for_chat=0 for all models
    pub async fn execute(&self) -> Result<()> {
        self.repository.clear_active_chat_model().await?;
        info!("Active chat model cleared");
        Ok(())
    }
}
