use async_trait::async_trait;
use lattice_core::shared::error::Result;

#[async_trait]
pub trait TagRepositoryTrait: Send + Sync {
    /// Create a new tag.
    async fn create_tag(
        &self,
        name: &str,
        color: Option<&str>,
    ) -> Result<lattice_core::domain::entities::Tag>;

    /// Find tag by name (case-insensitive).
    async fn find_by_name(
        &self,
        name: &str,
    ) -> Result<Option<lattice_core::domain::entities::Tag>>;

    /// Get or create a tag by name (case-insensitive).
    async fn get_or_create(
        &self,
        name: &str,
        color: Option<&str>,
    ) -> Result<lattice_core::domain::entities::Tag>;

    /// Find tag by ID.
    async fn find_by_id(&self, id: &str) -> Result<Option<lattice_core::domain::entities::Tag>>;

    /// Find tags by filter criteria.
    async fn find_by_filter(
        &self,
        filter: &dyn lattice_core::application::ports::Filter,
    ) -> Result<Vec<lattice_core::domain::entities::Tag>>;

    /// Find all tags ordered by name.
    async fn find_all(&self) -> Result<Vec<lattice_core::domain::entities::Tag>>;

    /// Save tag entity (insert or update).
    async fn save(&self, entity: &lattice_core::domain::entities::Tag) -> Result<()>;

    /// Save multiple tag entities in a transaction.
    async fn save_batch(&self, entities: &[lattice_core::domain::entities::Tag]) -> Result<()>;

    /// Delete tag by ID.
    async fn delete(&self, id: &str) -> Result<()>;

    /// Delete multiple tags by ID in a transaction.
    async fn delete_batch(&self, ids: &[&str]) -> Result<()>;

    /// Count total number of tags.
    async fn count(&self) -> Result<usize>;

    /// Check if tag exists by ID.
    async fn exists(&self, id: &str) -> Result<bool>;

    /// Get all tags for a document.
    async fn get_tags_for_document(
        &self,
        document_id: &str,
    ) -> Result<Vec<lattice_core::domain::entities::Tag>>;

    /// Add tag to document (idempotent).
    async fn add_tag_to_document(&self, document_id: &str, tag_id: &str) -> Result<()>;

    /// Remove tag from document.
    async fn remove_tag_from_document(&self, document_id: &str, tag_id: &str) -> Result<()>;

    /// Find document IDs that have this tag.
    async fn find_documents_by_tag(&self, tag_id: &str) -> Result<Vec<String>>;

    /// Find document IDs that have a tag with this name (case-insensitive).
    async fn find_documents_by_tag_name(&self, tag_name: &str) -> Result<Vec<String>>;

    /// Get all tags with document counts ordered by name.
    async fn get_all_with_counts(&self) -> Result<Vec<(lattice_core::domain::entities::Tag, i64)>>;

    /// Update tag name and/or color.
    async fn update(
        &self,
        tag_id: &str,
        name: Option<&str>,
        color: Option<&str>,
    ) -> Result<lattice_core::domain::entities::Tag>;

    /// Add multiple tags to a document by name, creating any missing tags.
    async fn add_tags_to_document_by_names(
        &self,
        document_id: &str,
        tag_names: Vec<String>,
    ) -> Result<Vec<lattice_core::domain::entities::Tag>>;

    /// Get tags for multiple documents in a single batch query.
    async fn get_tags_for_documents(
        &self,
        document_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<lattice_core::domain::entities::Tag>>>;

    /// Remove all tags from a document.
    async fn remove_all_tags_from_document(&self, document_id: &str) -> Result<()>;

    /// Get or create tags in batch.
    async fn get_or_create_batch(
        &self,
        tag_names: Vec<String>,
    ) -> Result<Vec<lattice_core::domain::entities::Tag>>;

    /// Bulk add tags to multiple documents.
    async fn add_tags_to_documents_batch(
        &self,
        document_tags: Vec<(String, Vec<String>)>,
    ) -> Result<usize>;

    /// Get all tags (alias for find_all).
    async fn get_all(&self) -> Result<Vec<lattice_core::domain::entities::Tag>> {
        self.find_all().await
    }
}
