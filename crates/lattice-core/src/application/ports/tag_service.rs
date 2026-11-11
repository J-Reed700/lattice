//! Tag management as the tools and file metadata use it, and the per-document
//! lock that serialises tag writes.

use async_trait::async_trait;

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

    /// An uncontended guard for mocks.
    #[cfg(any(test, feature = "test-support"))]
    #[allow(clippy::expect_used)]
    pub fn mock() -> Self {
        let lock = std::sync::Arc::new(tokio::sync::Mutex::new(()));
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
    /// # Errors
    /// - `AppError::InvalidInput` if name is invalid
    /// - `AppError::Database` if database operation fails
    async fn create_tag(&self, name: &str, color: Option<&str>) -> Result<Tag>;

    /// Update an existing tag
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
    /// # Errors
    /// - `AppError::Database` if database operation fails
    async fn delete_tag(&self, tag_id: &str) -> Result<()>;

    /// Get all tags
    ///
    /// # Errors
    /// - `AppError::Database` if database query fails
    async fn get_all_tags(&self) -> Result<Vec<Tag>>;

    /// Get all tags with document counts
    ///
    /// # Errors
    /// - `AppError::Database` if database query fails
    async fn get_all_tags_with_counts(&self) -> Result<Vec<TagWithCountDto>>;

    /// Get tags for a document
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
    /// # Errors
    /// - `AppError::Database` if database operation fails
    async fn apply_tags(&self, document_id: &str, tag_names: Vec<String>) -> Result<Vec<Tag>>;

    /// Replace all document tags atomically. A failed replacement preserves the old set.
    async fn replace_tags(&self, document_id: &str, tag_names: Vec<String>) -> Result<()>;

    /// Remove a tag from a document
    ///
    /// # Errors
    /// - `AppError::Database` if database operation fails
    async fn remove_tag_from_document(&self, document_id: &str, tag_id: &str) -> Result<()>;

    /// Search documents by tag name
    ///
    /// # Errors
    /// - `AppError::Database` if database query fails
    async fn search_documents_by_tag(&self, tag_name: &str) -> Result<Vec<String>>;

    /// Acquire the exclusive per-document lock for tag and file-metadata writes.
    ///
    /// # Errors
    /// - `AppError::Other` if lock acquisition times out (30 seconds)
    async fn acquire_lock_with_timeout(&self, document_id: &str) -> Result<DocumentLockGuard>;
}
