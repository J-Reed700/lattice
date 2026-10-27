//! Downloaded Model Repository Port
//!
//! Domain-level abstraction for downloaded model persistence operations.
//! This trait defines the contract that infrastructure implementations must fulfill.
//!
//! # Bounded Context
//!
//! This trait represents the **Registry/Usage** bounded context in the model lifecycle.
//! It operates on `DownloadedModel` entities, which represent models that are:
//! - Successfully downloaded to local storage
//! - Tracked in the database
//! - Available for use in the application
//!
//! This is distinct from the `ModelRepository` trait, which represents the
//! **Acquisition** bounded context and operates on `Model` entities.
//!
//! # DDD Principles
//!
//! - **Dependency Inversion**: Application layer depends on this abstraction, not concrete implementations
//! - **Bounded Context Separation**: Keeps acquisition and usage concerns separated
//! - **Port/Adapter Pattern**: This is a "port" that infrastructure "adapters" implement

use crate::domain::models::downloaded::DownloadedModel;
use crate::shared::error::Result;
use async_trait::async_trait;

/// Repository abstraction for downloaded model persistence
///
/// This trait defines the contract for managing downloaded model records
/// in persistent storage. It focuses on the "registry" aspect of model
/// management - tracking what models are downloaded and available.
///
/// # Implementing Types
///
/// The SQLite adapter implements this contract in the persistence layer.
#[async_trait]
pub trait DownloadedModelRepository: Send + Sync {
    /// Delete a downloaded model by its model_id
    ///
    /// # Errors
    ///
    /// Returns error if the database operation fails
    async fn delete(&self, model_id: &str) -> Result<()>;

    /// Find a downloaded model by its model_id
    ///
    /// # Errors
    ///
    /// Returns error if the database operation fails
    async fn find_by_model_id(&self, model_id: &str) -> Result<Option<DownloadedModel>>;

    /// List all downloaded models
    ///
    /// # Errors
    ///
    /// Returns error if the database operation fails
    async fn list(&self) -> Result<Vec<DownloadedModel>>;

    /// Delete a downloaded model by model_id if it's not the active model
    ///
    /// # Errors
    ///
    /// Returns error if the database operation fails
    async fn delete_by_model_id_if_not_active(
        &self,
        model_id: &str,
        active_model_id: &str,
    ) -> Result<bool>;

    /// Find a downloaded model by its record id (not its model_id)
    async fn find_by_id(&self, id: &str) -> Result<Option<DownloadedModel>>;

    /// Insert or update a downloaded model record
    ///
    /// The stored embedding artifact identity is written as the model
    /// carries it, so a freshly constructed model clears it.
    async fn save(&self, model: &DownloadedModel) -> Result<()>;

    /// Whether a model with this model_id is recorded as fully downloaded
    async fn is_downloaded(&self, model_id: &str) -> Result<bool>;

    /// Whether any embedding model is registered and downloaded
    async fn has_any_embedding_model(&self) -> Result<bool>;

    /// Delete a downloaded model by its record id unless it is active for
    /// chat or embedding, in one atomic statement
    ///
    /// # Errors
    ///
    /// `InvalidInput` if the model is active for chat or embedding
    async fn delete_if_not_active(&self, id: &str) -> Result<Option<DownloadedModel>>;
}
