//! Sets the active utility model (HyDE expansion, router/intent classification).

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

    pub async fn execute(&self, model_id: &str) -> Result<()> {
        let model = self
            .repository
            .find_by_model_id(model_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Model not found: {}", model_id)))?;

        // Local models gate on the download check; remote (Ollama) models don't have on-disk files.
        if model.location().is_local() && !self.repository.is_downloaded(model_id).await? {
            return Err(AppError::InvalidInput(format!(
                "Model '{}' is not fully downloaded yet. Finish the download before activating it.",
                model_id
            )));
        }
        // Utility is a chat-style generation role -- reject embedding the same way the chat slot does.
        model.validate_for_operation(true)?;

        self.repository.set_active_utility_model(model_id).await?;
        info!(model_id = %model_id, "Active utility model updated");
        Ok(())
    }

    /// Clear the utility model slot, reverting HyDE/router to the chat model.
    pub async fn clear(&self) -> Result<()> {
        self.repository.clear_active_utility_model().await?;
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
    async fn ollama_row_is_accepted_as_utility() {
        let repo = setup_repo().await;
        let use_case = SetActiveUtilityModelUseCase::new(repo.clone());

        use_case
            .execute("__ollama_server__")
            .await
            .expect("Ollama utility model should be accepted");

        let active = repo
            .get_active_utility_model()
            .await
            .expect("get active utility");
        assert!(active.is_some(), "utility slot should hold the Ollama row");
    }

    #[tokio::test]
    async fn clear_removes_active_utility_assignment() {
        let repo = setup_repo().await;
        let use_case = SetActiveUtilityModelUseCase::new(repo.clone());

        repo.set_active_utility_model("__ollama_server__")
            .await
            .expect("seed utility slot");
        use_case.clear().await.expect("clear utility");

        let active = repo
            .get_active_utility_model()
            .await
            .expect("get active utility");
        assert!(active.is_none(), "utility should be cleared");
    }

    #[tokio::test]
    async fn returns_not_found_for_unknown_model_id() {
        let repo = setup_repo().await;
        let use_case = SetActiveUtilityModelUseCase::new(repo);

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
