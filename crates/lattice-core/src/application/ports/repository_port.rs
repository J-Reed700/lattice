//! Repository port for generic entity persistence.

use crate::shared::error::Result;
use async_trait::async_trait;

/// Generic repository interface for domain entity CRUD operations.
#[async_trait]
pub trait RepositoryPort<T>: Send + Sync
where
    T: Send + Sync,
{
    /// Find an entity by its unique identifier.
    async fn find_by_id(&self, id: &str) -> Result<Option<T>>;

    /// Find all entities matching a filter.
    async fn find_by_filter(&self, filter: &dyn Filter) -> Result<Vec<T>>;

    /// Retrieve all entities.
    async fn find_all(&self) -> Result<Vec<T>>;

    /// Save an entity (insert or update).
    async fn save(&self, entity: &T) -> Result<()>;

    /// Save multiple entities in a batch operation.
    async fn save_batch(&self, entities: &[T]) -> Result<()>;

    /// Delete an entity by its unique identifier.
    async fn delete(&self, id: &str) -> Result<()>;

    /// Delete multiple entities by their identifiers.
    async fn delete_batch(&self, ids: &[&str]) -> Result<()>;

    /// Count the total number of entities.
    async fn count(&self) -> Result<usize>;

    /// Check if an entity with the given ID exists.
    async fn exists(&self, id: &str) -> Result<bool>;
}

/// Trait for repository filters.
pub trait Filter: Send + Sync + 'static {
    fn validate(&self) -> Result<()>;
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Empty filter that matches all entities.
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
