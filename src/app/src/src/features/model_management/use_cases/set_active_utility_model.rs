//! Set Active Utility Model Use Case
//!
//! Sets which model to use for HyDE expansion, router/intent classification.
//! The utility model is typically a small fast instruct model like
//! `llama3.1:8b` or `qwen2.5:7b` — using a 27B reasoning model here costs
//! many seconds per turn for near-zero quality gain.

use crate::domain::downloaded_model::ModelBackend;
use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::shared::error::{AppError, Result};
use tracing::info;

pub struct SetActiveUtilityModelUseCase {
    repository: DownloadedModelRepository,
}

impl SetActiveUtilityModelUseCase {
    pub fn new(repository: DownloadedModelRepository) -> Self {
        Self { repository }
    }

    /// Set a model active for the utility role.
    ///
    /// Local models must be fully downloaded. Ollama-backed models are
    /// considered "present" if they appear in the model table (populated
    /// by the Ollama sync service) — no disk-file check applies.
    pub async fn execute(&self, model_id: &str) -> Result<()> {
        let model = self
            .repository
            .find_by_model_id(model_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Model not found: {}", model_id)))?;

        if model.backend() == ModelBackend::Local {
            if !self.repository.is_downloaded(model_id).await? {
                return Err(AppError::InvalidInput(format!(
                    "Model '{}' is not fully downloaded yet. Finish the download before activating it.",
                    model_id
                )));
            }
            // Utility is a chat-style generation role — reject non-LLM picks
            // the same way chat does (rejects embedding models, etc.).
            model.validate_for_operation(true)?;
        }

        self.repository.set_active_utility_model(model_id).await?;
        info!(model_id = %model_id, "Active utility model updated");
        Ok(())
    }

    /// Clear the utility model slot — reverting HyDE/router to the chat model.
    pub async fn clear(&self) -> Result<()> {
        self.repository.clear_active_utility_model().await?;
        Ok(())
    }
}
