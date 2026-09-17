//! Sets the active embedding model.
//!
//! Activation is where a local model's artifact identity is established: the
//! identity is computed (or reused from the row) here and recorded with the
//! activation, so launch never hashes model files.

use crate::domain::value_objects::ArtifactIdentity;
use crate::features::embedding::artifact_identity;
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

    /// Validate and activate `model_id`, returning the identity it was
    /// activated with (`None` for remote models).
    pub async fn execute(&self, model_id: &str) -> Result<Option<ArtifactIdentity>> {
        let identity = self.establish(model_id).await?;
        self.activate(model_id, identity.as_ref()).await?;
        Ok(identity)
    }

    /// Run the activation checks and return the identity `model_id` must be
    /// activated with, computing it off the runtime thread if the row has
    /// none. Writes nothing; callers that need to prepare the model's vector
    /// space before switching to it call this, then `activate`.
    pub async fn establish(&self, model_id: &str) -> Result<Option<ArtifactIdentity>> {
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

        artifact_identity::establish(&model).await
    }

    /// Record the activation. `identity` comes from `establish`.
    pub async fn activate(
        &self,
        model_id: &str,
        identity: Option<&ArtifactIdentity>,
    ) -> Result<()> {
        self.repository
            .set_active_embedding_model(model_id, identity)
            .await?;

        info!(model_id = %model_id, "Active embedding model updated");
        Ok(())
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
mod tests {
    use super::*;
    use crate::domain::downloaded_model::{DownloadedModel, ModelLocation};
    use sqlx::sqlite::SqlitePoolOptions;
    use std::path::Path;

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

    fn embedding_model_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create model directory");
        for (name, contents) in [
            ("config.json", &b"{}"[..]),
            ("tokenizer.json", &b"{}"[..]),
            ("model.safetensors", &b"weights"[..]),
        ] {
            std::fs::write(dir.path().join(name), contents).expect("write model file");
        }
        dir
    }

    async fn save_directory_model(repo: &DownloadedModelRepository, model_id: &str, dir: &Path) {
        let model = DownloadedModel::new(
            uuid::Uuid::new_v4().to_string(),
            format!("Embed {model_id}"),
            model_id.to_string(),
            ModelLocation::LocalDirectory {
                path: dir.to_path_buf(),
            },
            7,
            "bert".to_string(),
            None,
        )
        .expect("create embedding model");
        repo.save(&model).await.expect("save embedding model");
    }

    async fn stored_identity(
        repo: &DownloadedModelRepository,
        model_id: &str,
    ) -> Option<ArtifactIdentity> {
        repo.find_by_model_id(model_id)
            .await
            .expect("find model")
            .expect("model exists")
            .embedding_artifact_identity()
            .cloned()
    }

    #[tokio::test]
    async fn activation_stores_the_computed_identity() {
        let repo = setup_repo().await;
        let dir = embedding_model_dir();
        save_directory_model(&repo, "local-embed", dir.path()).await;
        let expected = artifact_identity::compute(dir.path()).expect("compute identity");

        let returned = SetActiveEmbeddingModelUseCase::new(repo.clone())
            .execute("local-embed")
            .await
            .expect("activate local model");

        assert_eq!(returned.as_ref(), Some(&expected));
        assert_eq!(stored_identity(&repo, "local-embed").await, Some(expected));
        let active = repo
            .get_active_embedding_model()
            .await
            .expect("read active model")
            .expect("active embedding model");
        assert_eq!(active.model_id(), "local-embed");
    }

    /// Model artifacts are immutable after download: activation trusts the
    /// stored identity and never rehashes. Replacing files by hand is not
    /// detected here.
    #[tokio::test]
    async fn reactivation_reuses_the_stored_identity_after_files_change() {
        let repo = setup_repo().await;
        let dir = embedding_model_dir();
        save_directory_model(&repo, "local-embed", dir.path()).await;
        let use_case = SetActiveEmbeddingModelUseCase::new(repo.clone());
        let first = use_case
            .execute("local-embed")
            .await
            .expect("first activation")
            .expect("local identity");
        use_case
            .execute("__ollama_server__")
            .await
            .expect("switch away");

        std::fs::write(dir.path().join("model.safetensors"), b"replaced weights")
            .expect("replace weights");
        assert_ne!(
            artifact_identity::compute(dir.path()).expect("compute identity"),
            first,
            "the files did change"
        );

        let second = use_case
            .execute("local-embed")
            .await
            .expect("second activation");

        assert_eq!(second.as_ref(), Some(&first));
        assert_eq!(stored_identity(&repo, "local-embed").await, Some(first));
    }

    #[tokio::test]
    async fn establish_computes_without_writing() {
        let repo = setup_repo().await;
        let dir = embedding_model_dir();
        save_directory_model(&repo, "local-embed", dir.path()).await;

        let established = SetActiveEmbeddingModelUseCase::new(repo.clone())
            .establish("local-embed")
            .await
            .expect("establish identity");

        assert_eq!(
            established,
            Some(artifact_identity::compute(dir.path()).expect("compute identity"))
        );
        let model = repo
            .find_by_model_id("local-embed")
            .await
            .expect("find model")
            .expect("model exists");
        assert!(!model.is_active_for_embedding());
        assert_eq!(model.embedding_artifact_identity(), None);
        assert!(repo
            .get_active_embedding_model()
            .await
            .expect("read active model")
            .is_none());
    }

    #[tokio::test]
    async fn ollama_backed_model_bypasses_filesystem_and_type_checks() {
        // Synthetic Ollama row has model_type='chat' and zero files -- downloads gated behind Local.
        let repo = setup_repo().await;
        let use_case = SetActiveEmbeddingModelUseCase::new(repo);

        let identity = use_case
            .execute("__ollama_server__")
            .await
            .expect("activating ollama row should bypass local-only validation");
        assert_eq!(identity, None, "remote models carry no artifact identity");
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
