//! Set Active Embedding Model Use Case
//!
//! Sets which downloaded model to use for embedding feature.

use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::shared::error::{AppError, Result};
use tracing::info;

/// Use case for setting the active embedding model
pub struct SetActiveEmbeddingModelUseCase {
    repository: DownloadedModelRepository,
}

impl SetActiveEmbeddingModelUseCase {
    pub fn new(repository: DownloadedModelRepository) -> Self {
        Self { repository }
    }

    /// Execute the use case
    ///
    /// # Arguments
    ///
    /// * `model_id` - The model_id to set as active for embedding
    ///
    /// # Business Logic
    ///
    /// - Validates that the model is an embedding model (not a chat model)
    /// - Sets is_active_for_embedding=1 for specified model
    /// - Database trigger automatically deactivates all other models
    ///
    /// # Errors
    ///
    /// - Returns NotFound if model_id doesn't exist
    /// - Returns InvalidInput if trying to use a chat model (.gguf) for embeddings
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

        // Validate that this is an embedding model
        model.validate_for_operation(false)?;

        // Set as active
        self.repository.set_active_embedding_model(model_id).await?;

        info!(model_id = %model_id, "Active embedding model updated");
        Ok(())
    }
}
