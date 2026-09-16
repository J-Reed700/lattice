//! Repository port for data persistence.
//!
//! This port defines the generic repository pattern for CRUD operations
//! on domain entities. Infrastructure implementations handle database access.
//!
//! # Purpose
//!
//! - Abstracts database implementation details
//! - Provides generic interface for entity persistence
//! - Enables testing with in-memory repositories
//! - Supports both relational and document stores
//!
//! # Infrastructure Implementations
//!
//! - `SqliteRepository<T>` - SQLite database persistence
//! - `InMemoryRepository<T>` - In-memory storage for testing
//! - `PostgresRepository<T>` - PostgreSQL database persistence (future)
//!
//! # Example Usage
//!
//! ```rust
//! use crate::application::ports::RepositoryPort;
//! use crate::domain::entities::Document;
//!
//! async fn save_document(
//!     repo: &impl RepositoryPort<Document>,
//!     doc: Document,
//! ) -> Result<()> {
//!     repo.save(&doc).await
//! }
//! ```

use crate::shared::result::Result;
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
    /// # Arguments
    ///
    /// * `id` - The unique identifier of the entity
    ///
    /// # Returns
    ///
    /// `Some(T)` if the entity exists, `None` if not found.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    /// - `AppError::InvalidInput` if ID format is invalid
    ///
    /// # Example
    ///
    /// ```rust
    /// if let Some(doc) = repo.find_by_id("doc-123").await? {
    ///     println!("Found document: {}", doc.title);
    /// }
    /// ```
    async fn find_by_id(&self, id: &str) -> Result<Option<T>>;

    /// Find all entities matching a filter.
    ///
    /// The filter type is implementation-specific and allows for
    /// flexible querying without exposing database details.
    ///
    /// # Arguments
    ///
    /// * `filter` - Filter criteria for the query
    ///
    /// # Returns
    ///
    /// A vector of matching entities, empty if none match.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    /// - `AppError::InvalidInput` if filter is malformed
    ///
    /// # Example
    ///
    /// ```rust
    /// let filter = DocumentFilter {
    ///     tags: vec!["rust".into()],
    ///     limit: Some(10),
    /// };
    /// let docs = repo.find_by_filter(&filter).await?;
    /// ```
    async fn find_by_filter(&self, filter: &dyn Filter) -> Result<Vec<T>>;

    /// Retrieve all entities.
    ///
    /// **Warning**: This can be expensive for large datasets. Consider
    /// using pagination or `find_by_filter` with limits.
    ///
    /// # Returns
    ///
    /// A vector containing all entities.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    ///
    /// # Example
    ///
    /// ```rust
    /// let all_docs = repo.find_all().await?;
    /// println!("Total documents: {}", all_docs.len());
    /// ```
    async fn find_all(&self) -> Result<Vec<T>>;

    /// Save an entity (insert or update).
    ///
    /// If the entity's ID already exists, it is updated. Otherwise, a new
    /// entity is inserted.
    ///
    /// # Arguments
    ///
    /// * `entity` - The entity to save
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if save operation fails
    /// - `AppError::InvalidInput` if entity fails validation
    /// - `AppError::Conflict` if there are constraint violations
    ///
    /// # Example
    ///
    /// ```rust
    /// let doc = Document::new("My Document", "content");
    /// repo.save(&doc).await?;
    /// ```
    async fn save(&self, entity: &T) -> Result<()>;

    /// Save multiple entities in a batch operation.
    ///
    /// More efficient than calling `save` repeatedly. This should be
    /// atomic - either all saves succeed or none do.
    ///
    /// # Arguments
    ///
    /// * `entities` - Slice of entities to save
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if batch save fails
    /// - `AppError::InvalidInput` if any entity fails validation
    ///
    /// # Example
    ///
    /// ```rust
    /// let docs = vec![doc1, doc2, doc3];
    /// repo.save_batch(&docs).await?;
    /// ```
    async fn save_batch(&self, entities: &[T]) -> Result<()>;

    /// Delete an entity by its unique identifier.
    ///
    /// If the entity does not exist, this is a no-op (returns Ok).
    ///
    /// # Arguments
    ///
    /// * `id` - The unique identifier of the entity to delete
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if deletion fails
    /// - `AppError::InvalidInput` if ID format is invalid
    ///
    /// # Example
    ///
    /// ```rust
    /// repo.delete("doc-123").await?;
    /// ```
    async fn delete(&self, id: &str) -> Result<()>;

    /// Delete multiple entities by their identifiers.
    ///
    /// This should be atomic - either all deletes succeed or none do.
    ///
    /// # Arguments
    ///
    /// * `ids` - Slice of identifiers to delete
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if batch deletion fails
    ///
    /// # Example
    ///
    /// ```rust
    /// repo.delete_batch(&["doc-1", "doc-2", "doc-3"]).await?;
    /// ```
    async fn delete_batch(&self, ids: &[&str]) -> Result<()>;

    /// Count the total number of entities.
    ///
    /// # Returns
    ///
    /// The total count of entities in the repository.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if count query fails
    ///
    /// # Example
    ///
    /// ```rust
    /// let count = repo.count().await?;
    /// println!("Repository contains {} entities", count);
    /// ```
    async fn count(&self) -> Result<usize>;

    /// Check if an entity with the given ID exists.
    ///
    /// # Arguments
    ///
    /// * `id` - The unique identifier to check
    ///
    /// # Returns
    ///
    /// `true` if an entity with this ID exists, `false` otherwise.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if existence check fails
    ///
    /// # Example
    ///
    /// ```rust
    /// if repo.exists("doc-123").await? {
    ///     println!("Document exists");
    /// }
    /// ```
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
