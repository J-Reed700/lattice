//! Repository port for data persistence.
//!
//! This port defines the generic repository pattern for CRUD operations
//! on domain entities. Infrastructure implementations handle database access.

use crate::shared::error::Result;
use async_trait::async_trait;

/// Port for repository-based data persistence.
///
/// Provides a generic CRUD interface for domain entities. Implementations
/// handle the translation between domain models and database storage.
///
/// # Type Parameters
///
/// * `T` - The domain entity type being persisted
///
/// # Implementations Must
///
/// - Maintain referential integrity
/// - Handle concurrent access safely
/// - Support transactions (where applicable)
/// - Be thread-safe (`Send + Sync`)
#[async_trait]
pub trait RepositoryPort<T>: Send + Sync
where
    T: Send + Sync,
{
    /// Find an entity by its unique identifier.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    /// - `AppError::InvalidInput` if ID format is invalid
    async fn find_by_id(&self, id: &str) -> Result<Option<T>>;

    /// Find all entities matching a filter.
    ///
    /// The filter type is implementation-specific and allows for
    /// flexible querying without exposing database details.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    /// - `AppError::InvalidInput` if filter is malformed
    async fn find_by_filter(&self, filter: &dyn Filter) -> Result<Vec<T>>;

    /// Retrieve all entities.
    ///
    /// **Warning**: This can be expensive for large datasets. Consider
    /// using pagination or `find_by_filter` with limits.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    async fn find_all(&self) -> Result<Vec<T>>;

    /// Save an entity (insert or update).
    ///
    /// If the entity's ID already exists, it is updated. Otherwise, a new
    /// entity is inserted.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if save operation fails
    /// - `AppError::InvalidInput` if entity fails validation
    /// - `AppError::Conflict` if there are constraint violations
    async fn save(&self, entity: &T) -> Result<()>;

    /// Save multiple entities in a batch operation.
    ///
    /// More efficient than calling `save` repeatedly. This should be
    /// atomic - either all saves succeed or none do.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if batch save fails
    /// - `AppError::InvalidInput` if any entity fails validation
    async fn save_batch(&self, entities: &[T]) -> Result<()>;

    /// Delete an entity by its unique identifier.
    ///
    /// If the entity does not exist, this is a no-op (returns Ok).
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if deletion fails
    /// - `AppError::InvalidInput` if ID format is invalid
    async fn delete(&self, id: &str) -> Result<()>;

    /// Delete multiple entities by their identifiers.
    ///
    /// This should be atomic - either all deletes succeed or none do.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if batch deletion fails
    async fn delete_batch(&self, ids: &[&str]) -> Result<()>;

    /// Count the total number of entities.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if count query fails
    async fn count(&self) -> Result<usize>;

    /// Check if an entity with the given ID exists.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if existence check fails
    async fn exists(&self, id: &str) -> Result<bool>;
}

/// Trait for repository filters.
///
/// Implementations define specific filter criteria for entity queries.
/// This allows type-safe filtering without exposing database details.
pub trait Filter: Send + Sync + 'static {
    /// Validate the filter criteria.
    ///
    /// # Errors
    ///
    /// Returns `AppError::InvalidInput` if filter is invalid.
    fn validate(&self) -> Result<()>;

    /// Cast to `Any` for downcasting to concrete filter types.
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Empty filter that matches all entities.
///
/// Useful as a default or for "get all" queries.
#[derive(Debug, Clone)]
pub struct NoFilter;

impl Filter for NoFilter {
    fn validate(&self) -> Result<()> {
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
