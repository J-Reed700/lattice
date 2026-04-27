//! First Run Setup Use Case
//!
//! Detects if this is the first run and if a default embedding model needs to be downloaded.
//!
//! # Purpose
//!
//! Checks the model registry (DB) for any completed embedding model. If none exist,
//! recommends downloading the default model (DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME).
//!
//! # Business Logic
//!
//! 1. Query the model registry for any embedding model with status='completed'.
//! 2. If none found, return needs_setup=true with recommendation.
//! 3. If at least one exists, return needs_setup=false.
//!
//! # Example
//!
//! ```rust,ignore
//! let use_case = CheckFirstRunStatusUseCase::new(repository);
//! let response = use_case.execute().await?;
//!
//! if response.needs_setup {
//!     println!("First run detected. Recommended model: {}", response.recommended_model_id.unwrap());
//! }
//! ```

use crate::domain::embedding_constants::DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME;
use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::shared::error::Result;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

// =============================================================================
// DTOs
// =============================================================================

/// Response DTO for first-run status check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirstRunStatusResponse {
    /// Whether first-run setup is needed
    pub needs_setup: bool,
    /// Recommended model ID if setup is needed
    pub recommended_model_id: Option<String>,
    /// Recommended model name for UI display
    pub recommended_model_name: Option<String>,
    /// Estimated download size in bytes
    pub estimated_size_bytes: Option<u64>,
}

// =============================================================================
// Use Case
// =============================================================================

/// Check first-run status use case
pub struct CheckFirstRunStatusUseCase {
    /// Repository used to query the model registry
    repository: DownloadedModelRepository,
}

impl CheckFirstRunStatusUseCase {
    /// Create a new instance
    ///
    /// # Arguments
    ///
    /// * `repository` - Downloaded model repository (registry source of truth)
    pub fn new(repository: DownloadedModelRepository) -> Self {
        Self { repository }
    }

    /// Execute the use case
    ///
    /// # Returns
    ///
    /// Response indicating if setup is needed and recommended model
    ///
    /// # Errors
    ///
    /// Returns error if the registry query fails
    pub async fn execute(&self) -> Result<FirstRunStatusResponse> {
        // Registry is the source of truth — filesystem heuristics drift and miss
        // models stored in subdirs (see Phase 3 fix: the welcome modal never
        // dismissed because a non-recursive top-level .onnx scan never matched
        // the per-model subdirectory layout).
        debug!("Checking first-run status via model registry");
        let has_embedding = self.repository.has_any_embedding_model().await?;
        if has_embedding {
            debug!("Embedding model present — not first run");
            Ok(FirstRunStatusResponse {
                needs_setup: false,
                recommended_model_id: None,
                recommended_model_name: None,
                estimated_size_bytes: None,
            })
        } else {
            info!("No embedding model in registry — first run detected");
            Ok(Self::needs_setup_response())
        }
    }

    /// Create response for first-run setup needed
    fn needs_setup_response() -> FirstRunStatusResponse {
        FirstRunStatusResponse {
            needs_setup: true,
            // Use curated model ID so it matches the model catalog and download flow.
            recommended_model_id: Some(DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME.to_string()),
            recommended_model_name: Some(format!(
                "{} (Embedding Model)",
                DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME
            )),
            estimated_size_bytes: Some(420_000_000),
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
mod tests {
    use super::*;
    use crate::domain::downloaded_model::{DownloadedModel, ModelBackend};
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

    fn make_local_embedding_model(model_id: &str) -> DownloadedModel {
        DownloadedModel::new(
            uuid::Uuid::new_v4().to_string(),
            format!("Embed {}", model_id),
            model_id.to_string(),
            std::path::PathBuf::from(format!("/tmp/{}.onnx", model_id)),
            512,
            "bge".to_string(),
            None,
            ModelBackend::Local,
        )
        .expect("create local embedding model")
    }

    fn make_local_chat_model(model_id: &str) -> DownloadedModel {
        DownloadedModel::new(
            uuid::Uuid::new_v4().to_string(),
            format!("Chat {}", model_id),
            model_id.to_string(),
            std::path::PathBuf::from(format!("/tmp/{}.gguf", model_id)),
            1024,
            "llama".to_string(),
            None,
            ModelBackend::Local,
        )
        .expect("create local chat model")
    }

    #[tokio::test]
    async fn test_first_run_when_registry_empty_needs_setup() {
        let repo = setup_repo().await;
        let use_case = CheckFirstRunStatusUseCase::new(repo);

        let response = use_case.execute().await.expect("execute should succeed");

        assert!(response.needs_setup);
        assert_eq!(
            response.recommended_model_id.as_deref(),
            Some(DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME)
        );
        assert!(response.recommended_model_name.is_some());
        assert!(response.estimated_size_bytes.is_some());
    }

    #[tokio::test]
    async fn test_not_first_run_when_embedding_model_in_registry() {
        let repo = setup_repo().await;
        let embedding = make_local_embedding_model("test-embed-1");
        repo.save(&embedding).await.expect("save embedding model");

        let use_case = CheckFirstRunStatusUseCase::new(repo);
        let response = use_case.execute().await.expect("execute should succeed");

        assert!(!response.needs_setup);
        assert!(response.recommended_model_id.is_none());
        assert!(response.recommended_model_name.is_none());
        assert!(response.estimated_size_bytes.is_none());
    }

    #[tokio::test]
    async fn test_first_run_when_only_non_embedding_models_in_registry() {
        let repo = setup_repo().await;
        let chat = make_local_chat_model("test-chat-1");
        repo.save(&chat).await.expect("save chat model");

        let use_case = CheckFirstRunStatusUseCase::new(repo);
        let response = use_case.execute().await.expect("execute should succeed");

        assert!(response.needs_setup);
        assert_eq!(
            response.recommended_model_id.as_deref(),
            Some(DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME)
        );
    }

    #[tokio::test]
    async fn test_first_run_when_embedding_model_present_but_status_not_completed() {
        // Validates that has_any_embedding_model filters by status='completed'.
        // We bypass save() (which forces 'completed') and insert a row with status='downloading'.
        let repo = setup_repo().await;

        let pool = {
            let p = SqlitePoolOptions::new()
                .max_connections(1)
                .connect(":memory:")
                .await
                .expect("create in-memory pool");
            sqlx::migrate!("./migrations")
                .run(&p)
                .await
                .expect("run migrations");
            p
        };

        sqlx::query(
            r#"
            INSERT INTO models (
                id, model_name, model_id, base_path, total_size_bytes, status,
                model_type, architecture
            )
            VALUES (?1, ?2, ?3, ?4, ?5, 'downloading', 'embedding', 'bge')
            "#,
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind("Pending Embed")
        .bind("pending-embed-1")
        .bind("/tmp/pending-embed-1")
        .bind(512_i64)
        .execute(&pool)
        .await
        .expect("insert pending embedding row");

        let pending_repo = DownloadedModelRepository::new(pool);
        let use_case = CheckFirstRunStatusUseCase::new(pending_repo);
        let response = use_case.execute().await.expect("execute should succeed");

        assert!(
            response.needs_setup,
            "embedding row with status='downloading' must not satisfy first-run check"
        );
        assert_eq!(
            response.recommended_model_id.as_deref(),
            Some(DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME)
        );

        // Sanity: empty `repo` still also reports needs_setup.
        let baseline = CheckFirstRunStatusUseCase::new(repo)
            .execute()
            .await
            .expect("baseline execute");
        assert!(baseline.needs_setup);
    }
}
