use async_trait::async_trait;
use sqlx::SqlitePool;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};

use crate::features::tags::TagServiceTrait;

/// A guard that holds an owned mutex guard
///
/// This uses OwnedMutexGuard which internally holds an Arc to the mutex,
/// eliminating lifetime issues and unsafe code.
pub struct DocumentLockGuard {
    _guard: tokio::sync::OwnedMutexGuard<()>,
}

const DOCUMENT_LOCK_STRIPES: usize = 64;

/// Fixed-size document lock table. Hash collisions may serialize unrelated
/// documents briefly, but memory usage remains bounded regardless of how many
/// document ids pass through a long-running process.
#[derive(Debug, Clone)]
pub(crate) struct DocumentLockTable {
    stripes: Arc<Vec<Arc<Mutex<()>>>>,
}

impl Default for DocumentLockTable {
    fn default() -> Self {
        Self {
            stripes: Arc::new(
                (0..DOCUMENT_LOCK_STRIPES)
                    .map(|_| Arc::new(Mutex::new(())))
                    .collect(),
            ),
        }
    }
}

impl DocumentLockTable {
    pub(crate) fn lock_for(&self, document_id: &str) -> Arc<Mutex<()>> {
        let mut hasher = DefaultHasher::new();
        document_id.hash(&mut hasher);
        let index = (hasher.finish() as usize) % self.stripes.len();
        self.stripes
            .get(index)
            .cloned()
            .unwrap_or_else(|| Arc::new(Mutex::new(())))
    }
}

impl DocumentLockGuard {
    /// Create a new DocumentLockGuard from an OwnedMutexGuard
    pub(crate) fn new(guard: tokio::sync::OwnedMutexGuard<()>) -> Self {
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

#[derive(Debug, Clone)]
pub struct TagService {
    document_locks: DocumentLockTable,
    pub(crate) db_pool: SqlitePool,
}

impl TagService {
    pub fn new(db_pool: SqlitePool) -> Self {
        Self {
            document_locks: DocumentLockTable::default(),
            db_pool,
        }
    }

    fn get_document_lock(&self, document_id: &str) -> Arc<Mutex<()>> {
        self.document_locks.lock_for(document_id)
    }

    pub async fn acquire_lock_with_timeout(
        &self,
        document_id: &str,
    ) -> Result<DocumentLockGuard, String> {
        let lock = self.get_document_lock(document_id);

        // Use lock_owned() to get an OwnedMutexGuard that owns an Arc to the mutex
        // This eliminates lifetime issues without unsafe code
        match timeout(Duration::from_secs(30), lock.lock_owned()).await {
            Ok(guard) => {
                let lock_guard = DocumentLockGuard::new(guard);
                Ok(lock_guard)
            }
            Err(_) => Err(
                "Tag operation timed out - another operation is in progress for this document"
                    .to_string(),
            ),
        }
    }

    /// Merge existing tags with generated tags, removing duplicates (case-insensitive)
    pub fn merge_tags(existing: Vec<String>, generated: Vec<String>) -> Vec<String> {
        use std::collections::HashSet;

        // Normalize to lowercase for deduplication
        let mut normalized = HashSet::new();
        let mut result = Vec::new();

        for tag in existing {
            let normalized_tag = tag.to_lowercase();
            if normalized.insert(normalized_tag) {
                result.push(tag);
            }
        }

        for tag in generated {
            let normalized_tag = tag.to_lowercase();
            if normalized.insert(normalized_tag) {
                result.push(tag.to_lowercase());
            }
        }

        result
    }
}

#[async_trait]
impl TagServiceTrait for TagService {
    async fn create_tag(
        &self,
        name: &str,
        color: Option<&str>,
    ) -> Result<crate::features::tags::entity::Tag, crate::shared::error::AppError> {
        use crate::features::tags::repository::TagRepository;

        if name.is_empty() || name.len() > 100 {
            return Err(crate::shared::error::AppError::InvalidInput(
                "Tag name must be 1-100 characters".into(),
            ));
        }

        let normalized_name = name.trim().to_lowercase();
        let tag_repo = TagRepository::new(self.db_pool.clone());

        tag_repo.create(&normalized_name, color).await.map_err(|e| {
            crate::shared::error::AppError::Database(format!("Failed to create tag: {}", e))
        })
    }

    async fn update_tag(
        &self,
        tag_id: &str,
        name: Option<&str>,
        color: Option<&str>,
    ) -> Result<crate::features::tags::entity::Tag, crate::shared::error::AppError> {
        use crate::features::tags::repository::TagRepository;

        if let Some(n) = name {
            if n.is_empty() || n.len() > 100 {
                return Err(crate::shared::error::AppError::InvalidInput(
                    "Tag name must be 1-100 characters".into(),
                ));
            }
        }

        let normalized_name = name.map(|n| n.trim().to_lowercase());
        let tag_repo = TagRepository::new(self.db_pool.clone());

        tag_repo
            .update(tag_id, normalized_name.as_deref(), color)
            .await
            .map_err(|e| {
                crate::shared::error::AppError::Database(format!("Failed to update tag: {}", e))
            })
    }

    async fn delete_tag(&self, tag_id: &str) -> Result<(), crate::shared::error::AppError> {
        use crate::features::tags::repository::TagRepository;

        let tag_repo = TagRepository::new(self.db_pool.clone());
        tag_repo.delete(tag_id).await.map_err(|e| {
            crate::shared::error::AppError::Database(format!("Failed to delete tag: {}", e))
        })
    }

    async fn get_all_tags(
        &self,
    ) -> Result<Vec<crate::features::tags::entity::Tag>, crate::shared::error::AppError> {
        use crate::features::tags::repository::TagRepository;

        let tag_repo = TagRepository::new(self.db_pool.clone());
        tag_repo.get_all().await.map_err(|e| {
            crate::shared::error::AppError::Database(format!("Failed to get tags: {}", e))
        })
    }

    async fn get_all_tags_with_counts(
        &self,
    ) -> Result<Vec<crate::features::tags::dto::TagWithCountDto>, crate::shared::error::AppError>
    {
        use crate::features::tags::repository::TagRepository;

        let tag_repo = TagRepository::new(self.db_pool.clone());
        let results = tag_repo.get_all_with_counts().await.map_err(|e| {
            crate::shared::error::AppError::Database(format!(
                "Failed to get tags with counts: {}",
                e
            ))
        })?;

        Ok(results
            .into_iter()
            .map(|(tag, count)| crate::features::tags::dto::TagWithCountDto {
                id: tag.id().to_string(),
                name: tag.name().to_string(),
                color: Some(tag.color().to_string()),
                created_at: tag.created_at().to_rfc3339(),
                document_count: count,
            })
            .collect())
    }

    async fn get_tags_for_document(
        &self,
        document_id: &str,
    ) -> Result<Vec<crate::features::tags::entity::Tag>, crate::shared::error::AppError> {
        use crate::features::tags::repository::TagRepository;

        let tag_repo = TagRepository::new(self.db_pool.clone());
        tag_repo
            .get_tags_for_document(document_id)
            .await
            .map_err(|e| {
                crate::shared::error::AppError::Database(format!(
                    "Failed to get document tags: {}",
                    e
                ))
            })
    }

    async fn apply_tags(
        &self,
        document_id: &str,
        tag_names: Vec<String>,
    ) -> Result<Vec<crate::features::tags::entity::Tag>, crate::shared::error::AppError> {
        use crate::features::tags::repository::TagRepository;

        let tag_repo = TagRepository::new(self.db_pool.clone());

        let existing_tags = tag_repo
            .get_tags_for_document(document_id)
            .await
            .unwrap_or_default();
        let existing_tag_names: Vec<String> =
            existing_tags.iter().map(|t| t.name().to_string()).collect();

        // Normalize and filter new tags
        let normalized_tags: Vec<String> = tag_names
            .into_iter()
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty() && t.len() <= 100)
            .collect();

        // Merge with existing tags
        let merged_tags = Self::merge_tags(existing_tag_names, normalized_tags);

        tag_repo
            .add_tags_to_document_by_names(document_id, merged_tags)
            .await
            .map_err(|e| {
                crate::shared::error::AppError::Database(format!("Failed to apply tags: {}", e))
            })?;

        tag_repo
            .get_tags_for_document(document_id)
            .await
            .map_err(|e| {
                crate::shared::error::AppError::Database(format!(
                    "Failed to get updated tags: {}",
                    e
                ))
            })
    }

    async fn remove_tag_from_document(
        &self,
        document_id: &str,
        tag_id: &str,
    ) -> Result<(), crate::shared::error::AppError> {
        use crate::features::tags::repository::TagRepository;

        let tag_repo = TagRepository::new(self.db_pool.clone());
        tag_repo
            .remove_tag_from_document(document_id, tag_id)
            .await
            .map_err(|e| {
                crate::shared::error::AppError::Database(format!("Failed to remove tag: {}", e))
            })
    }

    async fn search_documents_by_tag(
        &self,
        tag_name: &str,
    ) -> Result<Vec<String>, crate::shared::error::AppError> {
        use crate::features::tags::repository::TagRepository;

        let tag_repo = TagRepository::new(self.db_pool.clone());
        tag_repo
            .find_documents_by_tag_name(tag_name)
            .await
            .map_err(|e| {
                crate::shared::error::AppError::Database(format!(
                    "Failed to search documents by tag: {}",
                    e
                ))
            })
    }

    async fn generate_tags(
        &self,
        document_id: &str,
        max_tags: usize,
    ) -> Result<Vec<String>, crate::shared::error::AppError> {
        Err(crate::shared::error::AppError::ServiceNotAvailable(
            format!(
                "Tag generation requires AI model (document_id={}, max_tags={})",
                document_id, max_tags
            ),
        ))
    }

    async fn auto_tag_all_documents(
        &self,
        max_documents: usize,
    ) -> Result<usize, crate::shared::error::AppError> {
        Err(crate::shared::error::AppError::ServiceNotAvailable(
            format!(
                "Auto-tagging requires AI model (max_documents={})",
                max_documents
            ),
        ))
    }

    async fn acquire_lock_with_timeout(
        &self,
        document_id: &str,
    ) -> Result<DocumentLockGuard, crate::shared::error::AppError> {
        TagService::acquire_lock_with_timeout(self, document_id)
            .await
            .map_err(crate::shared::error::AppError::Other)
    }

    async fn acquire_lock(
        &self,
        document_id: &str,
    ) -> Result<DocumentLockGuard, crate::shared::error::AppError> {
        TagService::acquire_lock_with_timeout(self, document_id)
            .await
            .map_err(crate::shared::error::AppError::Other)
    }

    async fn get_or_create(
        &self,
        name: &str,
        color: &str,
    ) -> Result<crate::features::tags::entity::Tag, crate::shared::error::AppError> {
        use crate::features::tags::repository::TagRepository;

        if name.is_empty() || name.len() > 100 {
            return Err(crate::shared::error::AppError::InvalidInput(
                "Tag name must be 1-100 characters".into(),
            ));
        }

        let normalized_name = name.trim().to_lowercase();
        let tag_repo = TagRepository::new(self.db_pool.clone());
        tag_repo
            .get_or_create(&normalized_name, Some(color))
            .await
            .map_err(|e| {
                crate::shared::error::AppError::Database(format!("Failed to get tag: {}", e))
            })
    }

    async fn get_all_with_counts(
        &self,
    ) -> Result<Vec<crate::features::tags::dto::TagWithCountDto>, crate::shared::error::AppError>
    {
        self.get_all_tags_with_counts().await
    }

    fn merge_tags(&self, existing: Vec<String>, generated: Vec<String>) -> Vec<String> {
        let mut seen_lower = std::collections::HashSet::new();
        let mut result = Vec::new();

        for tag in existing {
            let lower = tag.to_lowercase();
            if seen_lower.insert(lower) {
                result.push(tag);
            }
        }

        for tag in generated {
            let lower = tag.to_lowercase();
            if seen_lower.insert(lower) {
                result.push(tag);
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_tags() {
        let existing = vec!["rust".to_string(), "programming".to_string()];
        let generated = vec![
            "RUST".to_string(),
            "tutorial".to_string(),
            "Programming".to_string(),
            "async".to_string(),
        ];

        let merged = TagService::merge_tags(existing, generated);

        assert_eq!(merged.len(), 4);
        assert!(merged.contains(&"rust".to_string()));
        assert!(merged.contains(&"programming".to_string()));
        assert!(merged.contains(&"tutorial".to_string()));
        assert!(merged.contains(&"async".to_string()));
    }

    #[test]
    fn test_merge_tags_empty() {
        let existing: Vec<String> = vec![];
        let generated = vec!["tag1".to_string(), "tag2".to_string()];

        let merged = TagService::merge_tags(existing, generated);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_merge_tags_all_duplicates() {
        let existing = vec!["tag1".to_string(), "tag2".to_string()];
        let generated = vec!["TAG1".to_string(), "Tag2".to_string()];

        let merged = TagService::merge_tags(existing, generated);
        assert_eq!(merged.len(), 2);
    }

    #[tokio::test]
    async fn test_concurrent_lock_acquisition() {
        let db_pool = SqlitePool::connect(":memory:")
            .await
            .expect("Failed to create in-memory pool");
        let service = TagService::new(db_pool);
        let document_id = "test-doc";

        let guard1 = service.acquire_lock_with_timeout(document_id).await;
        assert!(guard1.is_ok());

        let service_clone = service.clone();
        let doc_id_clone = document_id.to_string();

        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            service_clone.acquire_lock_with_timeout(&doc_id_clone).await
        });

        drop(guard1);

        let guard2 = handle.await.unwrap();
        assert!(guard2.is_ok());
    }
}
