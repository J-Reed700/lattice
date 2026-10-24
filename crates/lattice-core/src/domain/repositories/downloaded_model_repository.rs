//! Domain repository port for downloaded model persistence operations.

use crate::domain::models::downloaded::DownloadedModel;
use crate::shared::error::Result;
use async_trait::async_trait;

/// Repository abstraction for downloaded model persistence.
#[async_trait]
pub trait DownloadedModelRepository: Send + Sync {
    /// Delete a downloaded model by its model_id.
    async fn delete(&self, model_id: &str) -> Result<()>;

    /// Find a downloaded model by its model_id.
    async fn find_by_model_id(&self, model_id: &str) -> Result<Option<DownloadedModel>>;

    /// List all downloaded models.
    async fn list(&self) -> Result<Vec<DownloadedModel>>;

    /// Delete a downloaded model by model_id if it's not the active model.
    async fn delete_by_model_id_if_not_active(
        &self,
        model_id: &str,
        active_model_id: &str,
    ) -> Result<bool>;

    /// Find a downloaded model by its record id (not its model_id).
    async fn find_by_id(&self, id: &str) -> Result<Option<DownloadedModel>>;

    /// Insert or update a downloaded model record.
    async fn save(&self, model: &DownloadedModel) -> Result<()>;

    /// Whether a model with this model_id is recorded as fully downloaded.
    async fn is_downloaded(&self, model_id: &str) -> Result<bool>;

    /// Whether any embedding model is registered and downloaded.
    async fn has_any_embedding_model(&self) -> Result<bool>;

    /// Delete a downloaded model by its record id unless it is active for
    /// chat or embedding, in one atomic statement.
    async fn delete_if_not_active(&self, id: &str) -> Result<Option<DownloadedModel>>;
}
