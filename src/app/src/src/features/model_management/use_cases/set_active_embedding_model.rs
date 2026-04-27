//! Set Active Embedding Model Use Case
//!
//! Sets which downloaded model to use for embedding feature.

use crate::domain::downloaded_model::ModelBackend;
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
        let model = self
            .repository
            .find_by_model_id(model_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Model not found: {}", model_id)))?;

        // Remote-backed models (Ollama) have no on-disk files and the file-extension based
        // type check doesn't apply, so the download + chat/embedding gate is Local-only.
        if model.backend() == ModelBackend::Local {
            if !self.repository.is_downloaded(model_id).await? {
                return Err(AppError::InvalidInput(format!(
                    "Model '{}' is not fully downloaded yet. Finish downloading all files before activating it.",
                    model_id
                )));
            }
            model.validate_for_operation(false)?;
        }

        self.repository.set_active_embedding_model(model_id).await?;

        info!(model_id = %model_id, "Active embedding model updated");
        Ok(())
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_repo() -> DownloadedModelRepository {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .expect("create in-memory pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("run migrations");
        DownloadedModelRepository::new(pool)
    }

    #[tokio::test]
    async fn ollama_backed_model_bypasses_filesystem_and_type_checks() {
        // The synthetic Ollama row has model_type='chat' and zero files. The pre-Phase-2
        // implementation would reject it here on both `is_downloaded` AND
        // `validate_for_operation(false)`. Phase 2 gates both behind backend == Local.
        let repo = setup_repo().await;
        let use_case = SetActiveEmbeddingModelUseCase::new(repo);

        use_case
            .execute("__ollama_server__")
            .await
            .expect("activating ollama row should bypass local-only validation");
    }

    #[tokio::test]
    async fn returns_not_found_for_unknown_model_id() {
        let repo = setup_repo().await;
        let use_case = SetActiveEmbeddingModelUseCase::new(repo);

        let err = use_case
            .execute("model-that-does-not-exist")
            .await
            .expect_err("unknown model should fail");

        assert!(
            matches!(err, AppError::NotFound(_)),
            "expected NotFound, got: {:?}",
            err
        );
    }
}
