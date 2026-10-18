//! Clear Active Chat Model Use Case
//!
//! Deactivates the currently active chat model.

use lattice_core::application::ports::active_models::ActiveModels;
use lattice_core::shared::error::Result;
use std::sync::Arc;
use tracing::info;

/// Use case for clearing the active chat model
pub struct ClearActiveChatModelUseCase {
    active_models: Arc<dyn ActiveModels>,
}

impl ClearActiveChatModelUseCase {
    pub fn new(active_models: Arc<dyn ActiveModels>) -> Self {
        Self { active_models }
    }

    /// Execute the use case
    ///
    /// Sets is_active_for_chat=0 for all models
    pub async fn execute(&self) -> Result<()> {
        self.active_models.clear_active_chat_model().await?;
        info!("Active chat model cleared");
        Ok(())
    }
}
