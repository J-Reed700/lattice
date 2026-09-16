//! Sets the active chat model.

use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::shared::error::{AppError, Result};
use tracing::info;

pub struct SetActiveChatModelUseCase {
    repository: DownloadedModelRepository,
}

impl SetActiveChatModelUseCase {
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
            model.validate_for_operation(true)?;
        }

        self.repository.set_active_chat_model(model_id).await?;

        info!(model_id = %model_id, "Active chat model updated");
        Ok(())
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
mod tests {
    use super::*;
    use crate::domain::downloaded_model::ModelLocation;
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
    async fn activates_synthetic_ollama_row_without_filesystem_check() {
        // Synthetic Ollama row has zero on-disk files -- download check is Local-only.
        let repo = setup_repo().await;
        let use_case = SetActiveChatModelUseCase::new(repo.clone());

        use_case
            .execute("__ollama_server__")
            .await
            .expect("activating ollama row should bypass the download check");

        let active = repo
            .get_active_chat_model()
            .await
            .expect("get active chat model")
            .expect("expected ollama row to be active for chat");
        assert_eq!(active.model_id(), "__ollama_server__");
        assert_eq!(active.location(), &ModelLocation::RemoteOllama);
    }

    #[tokio::test]
    async fn returns_not_found_for_unknown_model_id() {
        let repo = setup_repo().await;
        let use_case = SetActiveChatModelUseCase::new(repo);

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
