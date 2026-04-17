//! Service trait definitions
//!
//! This module defines trait interfaces for dependency injection.

pub use crate::features::tags::service::DocumentLockGuard;
use crate::shared::error::Result;
use async_trait::async_trait;

#[async_trait]
pub trait TagServiceTrait: Send + Sync {
    /// Create a new tag
    ///
    /// # Arguments
    /// * `name` - Tag name (1-100 characters)
    /// * `color` - Optional hex color code
    ///
    /// # Returns
    /// Created tag
    ///
    /// # Errors
    /// - `AppError::InvalidInput` if name is invalid
    /// - `AppError::Database` if database operation fails
    async fn create_tag(&self, name: &str, color: Option<&str>) -> Result<crate::features::tags::entity::Tag>;

    /// Update an existing tag
    ///
    /// # Arguments
    /// * `tag_id` - Tag ID to update
    /// * `name` - Optional new name
    /// * `color` - Optional new color
    ///
    /// # Returns
    /// Updated tag
    ///
    /// # Errors
    /// - `AppError::InvalidInput` if name is invalid
    /// - `AppError::Database` if database operation fails
    async fn update_tag(
        &self,
        tag_id: &str,
        name: Option<&str>,
        color: Option<&str>,
    ) -> Result<crate::features::tags::entity::Tag>;

    /// Delete a tag
    ///
    /// # Arguments
    /// * `tag_id` - Tag ID to delete
    ///
    /// # Errors
    /// - `AppError::Database` if database operation fails
    async fn delete_tag(&self, tag_id: &str) -> Result<()>;

    /// Get all tags
    ///
    /// # Returns
    /// List of all tags
    ///
    /// # Errors
    /// - `AppError::Database` if database query fails
    async fn get_all_tags(&self) -> Result<Vec<crate::features::tags::entity::Tag>>;

    /// Get all tags with document counts
    ///
    /// # Returns
    /// List of tags with associated document counts, ordered by name
    ///
    /// # Errors
    /// - `AppError::Database` if database query fails
    async fn get_all_tags_with_counts(&self) -> Result<Vec<crate::features::tags::dto::TagWithCountDto>>;

    /// Get tags for a document
    ///
    /// # Arguments
    /// * `document_id` - Document ID
    ///
    /// # Returns
    /// List of tags associated with the document
    ///
    /// # Errors
    /// - `AppError::Database` if database query fails
    async fn get_tags_for_document(
        &self,
        document_id: &str,
    ) -> Result<Vec<crate::features::tags::entity::Tag>>;

    /// Apply tags to a document
    ///
    /// # Arguments
    /// * `document_id` - Document ID
    /// * `tag_names` - Tag names to apply
    ///
    /// # Returns
    /// Updated list of tags for the document
    ///
    /// # Errors
    /// - `AppError::Database` if database operation fails
    async fn apply_tags(
        &self,
        document_id: &str,
        tag_names: Vec<String>,
    ) -> Result<Vec<crate::features::tags::entity::Tag>>;

    /// Remove a tag from a document
    ///
    /// # Arguments
    /// * `document_id` - Document ID
    /// * `tag_id` - Tag ID to remove
    ///
    /// # Errors
    /// - `AppError::Database` if database operation fails
    async fn remove_tag_from_document(&self, document_id: &str, tag_id: &str) -> Result<()>;

    /// Search documents by tag name
    ///
    /// # Arguments
    /// * `tag_name` - Tag name to search for
    ///
    /// # Returns
    /// List of document IDs with the tag
    ///
    /// # Errors
    /// - `AppError::Database` if database query fails
    async fn search_documents_by_tag(&self, tag_name: &str) -> Result<Vec<String>>;

    /// Generate tags for a document using LLM
    ///
    /// **NOTE**: Currently disabled pending local LLM port
    ///
    /// # Arguments
    /// * `document_id` - Document ID
    /// * `max_tags` - Maximum number of tags to generate
    ///
    /// # Returns
    /// Generated tag names
    ///
    /// # Errors
    /// - `AppError::Other` - Feature disabled
    async fn generate_tags(&self, document_id: &str, max_tags: usize) -> Result<Vec<String>>;

    /// Auto-tag all documents
    ///
    /// **NOTE**: Currently disabled pending local LLM port
    ///
    /// # Arguments
    /// * `max_documents` - Maximum documents to tag
    ///
    /// # Returns
    /// Number of documents tagged
    ///
    /// # Errors
    /// - `AppError::Other` - Feature disabled
    async fn auto_tag_all_documents(&self, max_documents: usize) -> Result<usize>;

    /// Acquire exclusive lock for document tag operations with timeout
    ///
    /// Ensures only one tag operation can occur per document at a time,
    /// preventing race conditions during tag generation and merging.
    ///
    /// # Arguments
    /// * `document_id` - The document to lock
    ///
    /// # Returns
    /// Lock guard that releases the lock when dropped
    ///
    /// # Errors
    /// - `AppError::Other` if lock acquisition times out (30 seconds)
    async fn acquire_lock_with_timeout(&self, document_id: &str) -> Result<DocumentLockGuard>;

    /// Acquire exclusive lock for document tag operations (simpler version)
    ///
    /// Mock-friendly version without timeout parameter.
    ///
    /// # Arguments
    /// * `document_id` - The document to lock
    ///
    /// # Returns
    /// Lock guard that releases the lock when dropped
    ///
    /// # Errors
    /// - `AppError::Other` if lock acquisition fails
    async fn acquire_lock(&self, document_id: &str) -> Result<DocumentLockGuard>;

    /// Get or create a tag by name
    ///
    /// If tag with name exists, returns it. Otherwise creates new tag.
    ///
    /// # Arguments
    /// * `name` - Tag name
    /// * `color` - Tag color (hex code)
    ///
    /// # Returns
    /// Existing or newly created tag
    ///
    /// # Errors
    /// - `AppError::Database` if database operation fails
    async fn get_or_create(&self, name: &str, color: &str) -> Result<crate::features::tags::entity::Tag>;

    /// Get all tags with document counts (alternative to get_all_tags_with_counts)
    ///
    /// # Returns
    /// List of tags with associated document counts
    ///
    /// # Errors
    /// - `AppError::Database` if database query fails
    async fn get_all_with_counts(&self) -> Result<Vec<crate::features::tags::dto::TagWithCountDto>>;

    /// Merge existing and generated tags, removing duplicates (case-insensitive)
    ///
    /// # Arguments
    /// * `existing` - Tags already associated with document
    /// * `generated` - Newly generated tags to add
    ///
    /// # Returns
    /// Combined list with duplicates removed, preserving original case of existing tags
    ///
    /// # Example
    /// ```rust
    /// let existing = vec!["Rust".to_string()];
    /// let generated = vec!["rust".to_string(), "Programming".to_string()];
    /// let merged = service.merge_tags(existing, generated);
    /// // Result: ["Rust", "Programming"]
    /// ```
    fn merge_tags(&self, existing: Vec<String>, generated: Vec<String>) -> Vec<String>;
}

// ============================================================================
// File Storage Service Trait
// ============================================================================

/// Trait for file storage operations
///
/// Provides content-addressed file storage with deduplication and reference counting.
/// Files are stored using SHA256 hashes, enabling automatic dedup.
///
/// # Implementations
/// - `FileStorageService`: Production implementation with vault storage
/// - `MockFileStorageService`: In-memory mock for testing
#[async_trait]
#[async_trait]
pub trait TagRepositoryTrait: Send + Sync {
    /// Create a new tag (convenience method for tests)
    ///
    /// # Arguments
    /// * `name` - Tag name
    /// * `color` - Optional color
    ///
    /// # Returns
    /// The created tag as old models::Tag struct
    async fn create_tag(&self, name: &str, color: Option<&str>) -> Result<crate::features::tags::entity::Tag>;

    /// Find tag by name (case-insensitive).
    ///
    /// # Arguments
    /// * `name` - Tag name to search for
    ///
    /// # Returns
    /// `Some(TagEntity)` if found, `None` if not found
    async fn find_by_name(&self, name: &str) -> Result<Option<crate::features::tags::entity::Tag>>;

    /// Get or create a tag by name (case-insensitive).
    ///
    /// If a tag with the given name exists, it is returned. Otherwise, a new
    /// tag is created with the provided name and color.
    ///
    /// # Arguments
    /// * `name` - Tag name
    /// * `color` - Optional color (defaults to "#6366f1")
    ///
    /// # Returns
    /// The existing or newly created tag entity
    async fn get_or_create(
        &self,
        name: &str,
        color: Option<&str>,
    ) -> Result<crate::features::tags::entity::Tag>;

    /// Find tag by ID
    ///
    /// # Arguments
    /// * `id` - Tag ID (UUID string)
    ///
    /// # Returns
    /// `Some(TagEntity)` if found, `None` if not found
    async fn find_by_id(&self, id: &str) -> Result<Option<crate::features::tags::entity::Tag>>;

    /// Find tags by filter criteria
    ///
    /// # Arguments
    /// * `filter` - Filter implementation (e.g., TagFilter)
    ///
    /// # Returns
    /// Vector of matching tag entities
    async fn find_by_filter(
        &self,
        filter: &dyn crate::application::ports::Filter,
    ) -> Result<Vec<crate::features::tags::entity::Tag>>;

    /// Find all tags
    ///
    /// # Returns
    /// Vector of all tag entities ordered by name
    async fn find_all(&self) -> Result<Vec<crate::features::tags::entity::Tag>>;

    /// Save tag entity (insert or update)
    ///
    /// # Arguments
    /// * `entity` - Tag entity to save
    async fn save(&self, entity: &crate::features::tags::entity::Tag) -> Result<()>;

    /// Save multiple tag entities in a transaction
    ///
    /// # Arguments
    /// * `entities` - Slice of tag entities to save
    async fn save_batch(&self, entities: &[crate::features::tags::entity::Tag]) -> Result<()>;

    /// Delete tag by ID
    ///
    /// # Arguments
    /// * `id` - Tag ID to delete
    async fn delete(&self, id: &str) -> Result<()>;

    /// Delete multiple tags by ID in a transaction
    ///
    /// # Arguments
    /// * `ids` - Slice of tag IDs to delete
    async fn delete_batch(&self, ids: &[&str]) -> Result<()>;

    /// Count total number of tags
    ///
    /// # Returns
    /// Total count of tags
    async fn count(&self) -> Result<usize>;

    /// Check if tag exists by ID
    ///
    /// # Arguments
    /// * `id` - Tag ID to check
    ///
    /// # Returns
    /// `true` if tag exists, `false` otherwise
    async fn exists(&self, id: &str) -> Result<bool>;

    /// Get all tags for a document
    ///
    /// # Arguments
    /// * `document_id` - Document ID
    ///
    /// # Returns
    /// Vector of tags associated with the document
    async fn get_tags_for_document(
        &self,
        document_id: &str,
    ) -> Result<Vec<crate::features::tags::entity::Tag>>;

    /// Add tag to document (idempotent)
    ///
    /// # Arguments
    /// * `document_id` - Document ID
    /// * `tag_id` - Tag ID
    async fn add_tag_to_document(&self, document_id: &str, tag_id: &str) -> Result<()>;

    /// Remove tag from document
    ///
    /// # Arguments
    /// * `document_id` - Document ID
    /// * `tag_id` - Tag ID
    async fn remove_tag_from_document(&self, document_id: &str, tag_id: &str) -> Result<()>;

    /// Find documents by tag ID
    ///
    /// # Arguments
    /// * `tag_id` - Tag ID
    ///
    /// # Returns
    /// Vector of document IDs that have this tag
    async fn find_documents_by_tag(&self, tag_id: &str) -> Result<Vec<String>>;

    /// Find documents by tag name (case-insensitive)
    ///
    /// # Arguments
    /// * `tag_name` - Tag name
    ///
    /// # Returns
    /// Vector of document IDs that have a tag with this name
    async fn find_documents_by_tag_name(&self, tag_name: &str) -> Result<Vec<String>>;

    /// Get all tags with document counts
    ///
    /// # Returns
    /// Vector of tuples (tag, document_count) ordered by name
    async fn get_all_with_counts(&self) -> Result<Vec<(crate::features::tags::entity::Tag, i64)>>;

    /// Update tag (name and/or color)
    ///
    /// # Arguments
    /// * `tag_id` - Tag ID
    /// * `name` - Optional new name
    /// * `color` - Optional new color
    ///
    /// # Returns
    /// Updated tag entity
    async fn update(
        &self,
        tag_id: &str,
        name: Option<&str>,
        color: Option<&str>,
    ) -> Result<crate::features::tags::entity::Tag>;

    /// Add multiple tags to a document by tag names
    ///
    /// Creates tags if they don't exist.
    ///
    /// # Arguments
    /// * `document_id` - Document ID
    /// * `tag_names` - Tag names
    ///
    /// # Returns
    /// Vector of tags that were added
    async fn add_tags_to_document_by_names(
        &self,
        document_id: &str,
        tag_names: Vec<String>,
    ) -> Result<Vec<crate::features::tags::entity::Tag>>;

    /// Get tags for multiple documents (optimized batch query)
    ///
    /// # Arguments
    /// * `document_ids` - Document IDs
    ///
    /// # Returns
    /// HashMap mapping document IDs to their tags
    async fn get_tags_for_documents(
        &self,
        document_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<crate::features::tags::entity::Tag>>>;

    /// Remove all tags from a document
    ///
    /// # Arguments
    /// * `document_id` - Document ID
    async fn remove_all_tags_from_document(&self, document_id: &str) -> Result<()>;

    /// Get or create tags in batch
    ///
    /// # Arguments
    /// * `tag_names` - Tag names
    ///
    /// # Returns
    /// Vector of tag entities
    async fn get_or_create_batch(
        &self,
        tag_names: Vec<String>,
    ) -> Result<Vec<crate::features::tags::entity::Tag>>;

    /// Bulk add tags to multiple documents
    ///
    /// # Arguments
    /// * `document_tags` - Vector of (document_id, tag_names) pairs
    ///
    /// # Returns
    /// Number of tag associations created
    async fn add_tags_to_documents_batch(
        &self,
        document_tags: Vec<(String, Vec<String>)>,
    ) -> Result<usize>;

    /// Get all tags (alias for find_all)
    ///
    /// # Returns
    /// Vector of all tag entities ordered by name
    async fn get_all(&self) -> Result<Vec<crate::features::tags::entity::Tag>> {
        self.find_all().await
    }
}
