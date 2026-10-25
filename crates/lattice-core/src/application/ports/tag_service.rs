//! Tag management as the tools and file metadata use it, and the per-document
//! lock that serialises tag writes.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::application::contracts::tags::TagWithCountDto;
use crate::domain::entities::Tag;
use crate::shared::error::Result;

/// A guard that holds an owned mutex guard
///
/// This uses OwnedMutexGuard which internally holds an Arc to the mutex,
/// eliminating lifetime issues and unsafe code.
pub struct DocumentLockGuard {
    _guard: tokio::sync::OwnedMutexGuard<()>,
}

impl DocumentLockGuard {
    /// Create a new DocumentLockGuard from an OwnedMutexGuard
    pub fn new(guard: tokio::sync::OwnedMutexGuard<()>) -> Self {
        Self { _guard: guard }
    }

    /// Create mock lock guard for testing and mocks
    ///
    /// ⚠️ **TEST INFRASTRUCTURE ONLY**
    ///
    /// This mock is exposed outside `#[cfg(test)]` to enable cross-module testing.
    /// **Do not use in production code.** Use `DocumentLockGuard::new()` instead.
    ///
    /// # Panics
    ///
    /// Panics if lock acquisition fails (test infrastructure issue, not production code).
    #[allow(clippy::expect_used)] // Test infrastructure - panics are acceptable
    pub fn mock() -> Self {
        let lock = Arc::new(Mutex::new(()));
        // IMMACULATE INVARIANT: Test lock should always be available (no contention in tests)
        let guard = lock.clone().try_lock_owned().expect(
            "IMMACULATE INVARIANT: Mock lock guard should always acquire successfully in tests",
        );
        Self { _guard: guard }
    }
}

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
    async fn create_tag(&self, name: &str, color: Option<&str>) -> Result<Tag>;

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
    ) -> Result<Tag>;

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
    async fn get_all_tags(&self) -> Result<Vec<Tag>>;

    /// Get all tags with document counts
    ///
    /// # Returns
    /// List of tags with associated document counts, ordered by name
    ///
    /// # Errors
    /// - `AppError::Database` if database query fails
    async fn get_all_tags_with_counts(&self) -> Result<Vec<TagWithCountDto>>;

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
    async fn get_tags_for_document(&self, document_id: &str) -> Result<Vec<Tag>>;

    /// Tags for a set of documents, fetched in batches. Documents without tags
    /// may be absent from the returned map.
    async fn get_tags_for_documents(
        &self,
        document_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<Tag>>>;

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
    async fn apply_tags(&self, document_id: &str, tag_names: Vec<String>) -> Result<Vec<Tag>>;

    /// Replace all document tags atomically. A failed replacement preserves the old set.
    async fn replace_tags(&self, document_id: &str, tag_names: Vec<String>) -> Result<()>;

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
    async fn get_or_create(&self, name: &str, color: &str) -> Result<Tag>;

    /// Get all tags with document counts (alternative to get_all_tags_with_counts)
    ///
    /// # Returns
    /// List of tags with associated document counts
    ///
    /// # Errors
    /// - `AppError::Database` if database query fails
    async fn get_all_with_counts(&self) -> Result<Vec<TagWithCountDto>>;

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
