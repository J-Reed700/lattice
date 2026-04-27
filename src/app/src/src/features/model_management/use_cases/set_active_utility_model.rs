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
    async fn ollama_row_can_be_assigned_utility_role() {
        // Primary fix path: clicking "Set as Utility" on the Ollama Server card
        // should activate without erroring on the missing on-disk file.
        let repo = setup_repo().await;
        let use_case = SetActiveUtilityModelUseCase::new(repo.clone());

        use_case
            .execute("__ollama_server__")
            .await
            .expect("activating ollama row for utility should bypass download check");

        let active = repo
            .get_active_utility_model()
            .await
            .expect("get active utility")
            .expect("expected an active utility model");
        assert_eq!(active.model_id(), "__ollama_server__");
        assert_eq!(active.backend(), ModelBackend::Ollama);
    }

    #[tokio::test]
    async fn clear_removes_active_utility_assignment() {
        let repo = setup_repo().await;
        let use_case = SetActiveUtilityModelUseCase::new(repo.clone());

        use_case
            .execute("__ollama_server__")
            .await
            .expect("set utility");
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
