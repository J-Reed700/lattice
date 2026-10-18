//! Service trait definitions
//!
//! This module defines trait interfaces for dependency injection.

use async_trait::async_trait;
use lattice_core::shared::error::Result;

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
    async fn create_tag(
        &self,
        name: &str,
        color: Option<&str>,
    ) -> Result<lattice_core::domain::entities::Tag>;

    /// Find tag by name (case-insensitive).
    ///
    /// # Arguments
    /// * `name` - Tag name to search for
    ///
    /// # Returns
    /// `Some(TagEntity)` if found, `None` if not found
    async fn find_by_name(&self, name: &str)
        -> Result<Option<lattice_core::domain::entities::Tag>>;

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
    ) -> Result<lattice_core::domain::entities::Tag>;

    /// Find tag by ID
    ///
    /// # Arguments
    /// * `id` - Tag ID (UUID string)
    ///
    /// # Returns
    /// `Some(TagEntity)` if found, `None` if not found
    async fn find_by_id(&self, id: &str) -> Result<Option<lattice_core::domain::entities::Tag>>;

    /// Find tags by filter criteria
    ///
    /// # Arguments
    /// * `filter` - Filter implementation (e.g., TagFilter)
    ///
    /// # Returns
    /// Vector of matching tag entities
    async fn find_by_filter(
        &self,
        filter: &dyn lattice_core::application::ports::Filter,
    ) -> Result<Vec<lattice_core::domain::entities::Tag>>;

    /// Find all tags
    ///
    /// # Returns
    /// Vector of all tag entities ordered by name
    async fn find_all(&self) -> Result<Vec<lattice_core::domain::entities::Tag>>;

    /// Save tag entity (insert or update)
    ///
    /// # Arguments
    /// * `entity` - Tag entity to save
    async fn save(&self, entity: &lattice_core::domain::entities::Tag) -> Result<()>;

    /// Save multiple tag entities in a transaction
    ///
    /// # Arguments
    /// * `entities` - Slice of tag entities to save
    async fn save_batch(&self, entities: &[lattice_core::domain::entities::Tag]) -> Result<()>;

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
    ) -> Result<Vec<lattice_core::domain::entities::Tag>>;

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
    async fn get_all_with_counts(&self) -> Result<Vec<(lattice_core::domain::entities::Tag, i64)>>;

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
    ) -> Result<lattice_core::domain::entities::Tag>;

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
    ) -> Result<Vec<lattice_core::domain::entities::Tag>>;

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
    ) -> Result<std::collections::HashMap<String, Vec<lattice_core::domain::entities::Tag>>>;

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
    ) -> Result<Vec<lattice_core::domain::entities::Tag>>;

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
    async fn get_all(&self) -> Result<Vec<lattice_core::domain::entities::Tag>> {
        self.find_all().await
    }
}
