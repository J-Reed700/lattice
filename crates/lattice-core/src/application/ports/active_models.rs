//! Which downloaded models are active for chat, embedding and utility work.
//!
//! The records themselves are `domain::repositories::DownloadedModelRepository`;
//! this port reads and moves the active flags on those records. The SQLite
//! adapter implements both.

use async_trait::async_trait;

use crate::domain::models::downloaded::DownloadedModel;
use crate::domain::value_objects::ArtifactIdentity;
use crate::shared::error::Result;

/// The active chat, embedding and utility models.
///
/// The readers return only a model whose files are present: a local active
/// row whose download is incomplete reads as `None`. A remote (Ollama) row
/// has no local files to check and is returned as is.
#[async_trait]
pub trait ActiveModels: Send + Sync {
    /// The active chat model, if one is set and its files are present.
    async fn get_active_chat_model(&self) -> Result<Option<DownloadedModel>>;

    /// The active embedding model, if one is set and its files are present.
    async fn get_active_embedding_model(&self) -> Result<Option<DownloadedModel>>;

    /// The active utility model, if one is set and its files are present.
    async fn get_active_utility_model(&self) -> Result<Option<DownloadedModel>>;

    /// Make `model_id` the active chat model.
    async fn set_active_chat_model(&self, model_id: &str) -> Result<()>;

    /// Make `model_id` the active embedding model, storing `identity` when
    /// given.
    ///
    /// # Errors
    ///
    /// `NotFound` if `model_id` doesn't exist; `InvalidState` if it is a local
    /// model and neither `identity` nor the row carries an identity.
    async fn set_active_embedding_model(
        &self,
        model_id: &str,
        identity: Option<&ArtifactIdentity>,
    ) -> Result<()>;

    /// Make `model_id` the active utility model.
    async fn set_active_utility_model(&self, model_id: &str) -> Result<()>;

    /// Clear the active chat model.
    async fn clear_active_chat_model(&self) -> Result<()>;

    /// Clear the active embedding model.
    async fn clear_active_embedding_model(&self) -> Result<()>;

    /// Clear the active utility model.
    async fn clear_active_utility_model(&self) -> Result<()>;
}
