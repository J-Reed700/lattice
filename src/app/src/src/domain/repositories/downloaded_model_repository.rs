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

use crate::domain::downloaded_model::DownloadedModel;
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
/// See `crate::infrastructure::persistence::repositories::DownloadedModelRepository`
/// for the SQLite implementation.
#[async_trait]
pub trait DownloadedModelRepository: Send + Sync {
    /// Delete a downloaded model by its model_id
    ///
    /// # Arguments
    ///
    /// * `model_id` - The unique model identifier
    ///
    /// # Returns
    ///
    /// Ok(()) if the model was deleted successfully, or if it didn't exist
    ///
    /// # Errors
    ///
    /// Returns error if the database operation fails
    async fn delete(&self, model_id: &str) -> Result<()>;

    /// Find a downloaded model by its model_id
    ///
    /// # Arguments
    ///
    /// * `model_id` - The unique model identifier
    ///
    /// # Returns
    ///
    /// Some(DownloadedModel) if found, None otherwise
    ///
    /// # Errors
    ///
    /// Returns error if the database operation fails
    async fn find_by_model_id(&self, model_id: &str) -> Result<Option<DownloadedModel>>;

    /// List all downloaded models
    ///
    /// # Returns
    ///
    /// A vector of all downloaded models
    ///
    /// # Errors
    ///
    /// Returns error if the database operation fails
    async fn list(&self) -> Result<Vec<DownloadedModel>>;

    /// Delete a downloaded model by model_id if it's not the active model
    ///
    /// # Arguments
    ///
    /// * `model_id` - The unique model identifier
    /// * `active_model_id` - The currently active model ID (will not delete if matches)
    ///
    /// # Returns
    ///
    /// Ok(true) if deleted, Ok(false) if not deleted (because it's active)
    ///
    /// # Errors
    ///
    /// Returns error if the database operation fails
    async fn delete_by_model_id_if_not_active(
        &self,
        model_id: &str,
        active_model_id: &str,
    ) -> Result<bool>;
}
