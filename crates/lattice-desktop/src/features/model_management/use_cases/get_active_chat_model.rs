//! Get Active Chat Model Use Case
//!
//! Gets the currently active chat model.

use lattice_core::application::ports::active_models::ActiveModels;
use lattice_core::domain::models::downloaded::DownloadedModel;
use lattice_core::shared::error::Result;
use std::sync::Arc;

/// Use case for getting the active chat model
pub struct GetActiveChatModelUseCase {
    active_models: Arc<dyn ActiveModels>,
}

impl GetActiveChatModelUseCase {
    pub fn new(active_models: Arc<dyn ActiveModels>) -> Self {
        Self { active_models }
    }

    /// Execute the use case
    pub async fn execute(&self) -> Result<Option<DownloadedModel>> {
        self.active_models.get_active_chat_model().await
    }
}
