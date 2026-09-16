//! Sets the active embedding model.

use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::shared::error::{AppError, Result};
use tracing::info;

pub struct SetActiveEmbeddingModelUseCase {
    repository: DownloadedModelRepository,
}

impl SetActiveEmbeddingModelUseCase {
    pub fn new(repository: DownloadedModelRepository) -> Self {
        Self { repository }
    }

    pub async fn execute(&self, model_id: &str) -> Result<()> {
        let model = self
            .repository
            .find_by_model_id(model_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Model not found: {}", model_id)))?;

        // Remote-backed models have no on-disk files, so download + type checks only apply to Local.
        if model.location().is_local() {
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
        // Synthetic Ollama row has model_type='chat' and zero files -- downloads gated behind Local.
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
