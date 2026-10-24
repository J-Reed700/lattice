//! Tag management as the tools and file metadata use it, and the per-document
//! lock that serialises tag writes.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::application::contracts::tags::TagWithCountDto;
use crate::domain::entities::Tag;
use crate::shared::error::Result;

/// A guard that holds an owned mutex guard to serialize tag writes per document.
pub struct DocumentLockGuard {
    _guard: tokio::sync::OwnedMutexGuard<()>,
}

impl DocumentLockGuard {
    /// Create a new DocumentLockGuard from an OwnedMutexGuard.
    pub fn new(guard: tokio::sync::OwnedMutexGuard<()>) -> Self {
        Self { _guard: guard }
    }

    /// Create mock lock guard for testing and mocks.
    #[allow(clippy::expect_used)]
    pub fn mock() -> Self {
        let lock = Arc::new(Mutex::new(()));
        let guard = lock
            .clone()
            .try_lock_owned()
            .expect("Mock lock guard should always acquire successfully in tests");
        Self { _guard: guard }
    }
}

#[async_trait]
pub trait TagServiceTrait: Send + Sync {
    /// Create a new tag.
    async fn create_tag(&self, name: &str, color: Option<&str>) -> Result<Tag>;

    /// Update an existing tag's name or color.
    async fn update_tag(
        &self,
        tag_id: &str,
        name: Option<&str>,
        color: Option<&str>,
    ) -> Result<Tag>;

    /// Delete a tag by ID.
    async fn delete_tag(&self, tag_id: &str) -> Result<()>;

    /// Get all tags.
    async fn get_all_tags(&self) -> Result<Vec<Tag>>;

    /// Get all tags with associated document counts, ordered by name.
    async fn get_all_tags_with_counts(&self) -> Result<Vec<TagWithCountDto>>;

    /// Get tags for a document.
    async fn get_tags_for_document(&self, document_id: &str) -> Result<Vec<Tag>>;

    /// Apply tags to a document.
    async fn apply_tags(&self, document_id: &str, tag_names: Vec<String>) -> Result<Vec<Tag>>;

    /// Replace all document tags atomically. A failed replacement preserves the old set.
    async fn replace_tags(&self, document_id: &str, tag_names: Vec<String>) -> Result<()>;

    /// Remove a tag from a document.
    async fn remove_tag_from_document(&self, document_id: &str, tag_id: &str) -> Result<()>;

    /// Search document IDs that have the given tag.
    async fn search_documents_by_tag(&self, tag_name: &str) -> Result<Vec<String>>;

    /// Generate tags for a document using LLM.
    async fn generate_tags(&self, document_id: &str, max_tags: usize) -> Result<Vec<String>>;

    /// Auto-tag all documents.
    async fn auto_tag_all_documents(&self, max_documents: usize) -> Result<usize>;

    /// Acquire exclusive lock for document tag operations with timeout.
    async fn acquire_lock_with_timeout(&self, document_id: &str) -> Result<DocumentLockGuard>;

    /// Acquire exclusive lock for document tag operations.
    async fn acquire_lock(&self, document_id: &str) -> Result<DocumentLockGuard>;

    /// Get or create a tag by name.
    async fn get_or_create(&self, name: &str, color: &str) -> Result<Tag>;

    /// Get all tags with document counts.
    async fn get_all_with_counts(&self) -> Result<Vec<TagWithCountDto>>;

    /// Merge existing and generated tags, removing case-insensitive duplicates while preserving existing case.
    fn merge_tags(&self, existing: Vec<String>, generated: Vec<String>) -> Vec<String>;
}
