//! Set Active Chat Model Use Case
//!
//! Sets which downloaded model to use for chat feature.

use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::shared::error::{AppError, Result};
use tracing::info;

/// Use case for setting the active chat model
pub struct SetActiveChatModelUseCase {
    repository: DownloadedModelRepository,
}

impl SetActiveChatModelUseCase {
    pub fn new(repository: DownloadedModelRepository) -> Self {
        Self { repository }
    }

    /// Execute the use case
    ///
    /// # Arguments
    ///
    /// * `model_id` - The model_id to set as active for chat
    ///
    /// # Business Logic
    ///
    /// - Validates that the model is a chat model (not an embedding model)
    /// - Sets is_active_for_chat=1 for specified model
    /// - Database trigger automatically deactivates all other models
    ///
    /// # Errors
    ///
    /// - Returns NotFound if model_id doesn't exist
    /// - Returns InvalidInput if trying to use an embedding model (.onnx) for chat
    pub async fn execute(&self, model_id: &str) -> Result<()> {
        if !self.repository.is_downloaded(model_id).await? {
            return Err(AppError::InvalidInput(format!(
                "Model '{}' is not fully downloaded yet. Finish downloading all files before activating it.",
                model_id
            )));
        }

        // Fetch the model to validate its type
        let model = self
            .repository
            .find_by_model_id(model_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Model not found: {}", model_id)))?;

        // Validate that this is a chat model
        model.validate_for_operation(true)?;

        // Set as active
        self.repository.set_active_chat_model(model_id).await?;

        info!(model_id = %model_id, "Active chat model updated");
        Ok(())
    }
}
