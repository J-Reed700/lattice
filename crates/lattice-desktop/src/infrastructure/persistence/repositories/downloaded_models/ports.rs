//! The domain repository trait, `ActiveModels` and `ModelStoragePort` over the SQLite repository.

use async_trait::async_trait;
use tracing::{debug, error};

use lattice_core::application::ports::active_models::ActiveModels;
use lattice_core::application::ports::model_storage::{
    DownloadedModel as DownloadedModelDto, ModelStoragePort,
};
use lattice_core::domain::models::downloaded::DownloadedModel;
use lattice_core::domain::repositories::downloaded_model_repository::DownloadedModelRepository as DownloadedModelRepositoryTrait;
use lattice_core::domain::value_objects::ArtifactIdentity;
use lattice_core::shared::error::{AppError, Result};

use super::DownloadedModelRepository;

// Implement domain trait for dependency inversion

#[async_trait]
impl DownloadedModelRepositoryTrait for DownloadedModelRepository {
    /// Delete a downloaded model by its model_id
    ///
    /// This implements the domain trait contract by delegating to
    /// a SQL DELETE operation filtered by model_id.
    async fn delete(&self, model_id: &str) -> Result<()> {
        sqlx::query!(
            r#"
            DELETE FROM models
            WHERE model_id = ?1
            "#,
            model_id
        )
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id, "Failed to delete downloaded model by model_id");
            AppError::Database(format!("Failed to delete downloaded model: {}", e))
        })?;

        debug!(model_id = %model_id, "Downloaded model deleted by model_id");
        Ok(())
    }

    /// Find a downloaded model by its model_id
    ///
    /// This implements the domain trait contract by delegating to
    /// the existing find_by_model_id method.
    async fn find_by_model_id(&self, model_id: &str) -> Result<Option<DownloadedModel>> {
        // Delegate to existing method
        self.find_by_model_id(model_id).await
    }

    /// List all downloaded models
    ///
    /// This implements the domain trait contract by delegating to
    /// the existing list_all method.
    async fn list(&self) -> Result<Vec<DownloadedModel>> {
        self.list_all().await
    }

    /// Delete a downloaded model by model_id if it's not the active model.
    /// Returns Ok(true) if deleted, Ok(false) if not deleted (because it's active or doesn't exist).
    async fn delete_by_model_id_if_not_active(
        &self,
        model_id: &str,
        active_model_id: &str,
    ) -> Result<bool> {
        // Atomic DELETE with WHERE guard checking both model id and active flags.
        let result = sqlx::query(
            r#"
            DELETE FROM models
            WHERE model_id = ?1
              AND model_id != ?2
              AND is_active_for_chat = 0
              AND is_active_for_embedding = 0
            "#,
        )
        .bind(model_id)
        .bind(active_model_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, model_id = %model_id, "Failed to delete model if not active");
            AppError::Database(format!("Failed to delete model: {}", e))
        })?;

        Ok(result.rows_affected() > 0)
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<DownloadedModel>> {
        self.find_by_id(id).await
    }

    async fn save(&self, model: &DownloadedModel) -> Result<()> {
        self.save(model).await
    }

    async fn is_downloaded(&self, model_id: &str) -> Result<bool> {
        self.is_downloaded(model_id).await
    }

    async fn has_any_embedding_model(&self) -> Result<bool> {
        self.has_any_embedding_model().await
    }

    async fn delete_if_not_active(&self, id: &str) -> Result<Option<DownloadedModel>> {
        self.delete_if_not_active(id).await
    }
}

// The active flags, delegating to the SQL in `activation.rs`.

#[async_trait]
impl ActiveModels for DownloadedModelRepository {
    async fn get_active_chat_model(&self) -> Result<Option<DownloadedModel>> {
        self.get_active_chat_model().await
    }

    async fn get_active_embedding_model(&self) -> Result<Option<DownloadedModel>> {
        self.get_active_embedding_model().await
    }

    async fn get_active_utility_model(&self) -> Result<Option<DownloadedModel>> {
        self.get_active_utility_model().await
    }

    async fn set_active_chat_model(&self, model_id: &str) -> Result<()> {
        self.set_active_chat_model(model_id).await
    }

    async fn set_active_embedding_model(
        &self,
        model_id: &str,
        identity: Option<&ArtifactIdentity>,
    ) -> Result<()> {
        self.set_active_embedding_model(model_id, identity).await
    }

    async fn set_active_utility_model(&self, model_id: &str) -> Result<()> {
        self.set_active_utility_model(model_id).await
    }

    async fn clear_active_chat_model(&self) -> Result<()> {
        self.clear_active_chat_model().await
    }

    async fn clear_active_embedding_model(&self) -> Result<()> {
        self.clear_active_embedding_model().await
    }

    async fn clear_active_utility_model(&self) -> Result<()> {
        self.clear_active_utility_model().await
    }
}

// Implements the application port for use cases that need model storage operations.

#[async_trait::async_trait]
impl ModelStoragePort for DownloadedModelRepository {
    async fn list_models(&self) -> Result<Vec<DownloadedModelDto>> {
        let models = self.list().await?;

        Ok(models
            .into_iter()
            .map(|model| DownloadedModelDto {
                model_id: model.model_id().to_string(),
                path: model
                    .loadable_path()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_default(),
                size_bytes: model.file_size_bytes() as u64,
                downloaded_at: *model.downloaded_at(),
            })
            .collect())
    }

    async fn is_model_downloaded(&self, model_id: &str) -> Result<bool> {
        self.is_downloaded(model_id).await
    }

    async fn get_model_path(&self, model_id: &str) -> Result<std::path::PathBuf> {
        let model = self
            .find_by_model_id(model_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Model not found: {}", model_id)))?;

        let path = model
            .loadable_path()
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "Model {} has no local path (remote-hosted)",
                    model_id
                ))
            })?
            .to_path_buf();
        // Either a file (GGUF, single-shard) or a directory (safetensors layout).
        if !path.exists() {
            return Err(AppError::NotFound(format!(
                "Model artifact not found on disk: {}",
                path.display()
            )));
        }

        Ok(path)
    }

    async fn delete_model(&self, model_id: &str) -> Result<()> {
        // For ModelStoragePort, we use simple delete (without active check)
        // since this is used by use cases that have their own business logic
        self.delete(model_id).await
    }
}
