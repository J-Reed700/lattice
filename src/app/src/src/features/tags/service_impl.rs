//! Tag Service Implementation
//!
//! Full implementation of TagServiceTrait with all business logic
//! extracted from legacy commands.
//!
//! **NOTE**: Tag generation uses local LLM via TagServiceImpl.
//! See infrastructure/extraction/tag_generator.rs for prompt helpers.

use crate::application::ports::{LLMPort, RepositoryPort};
use crate::features::tags::generator::{
    DocumentMetadata, TagGenerator, TAG_GENERATION_SYSTEM_PROMPT,
};
use crate::infrastructure::persistence::repositories::{DocumentRepositoryImpl, TagRepository};
use crate::features::tags::service::DocumentLockGuard;
use crate::infrastructure::services::traits::TagServiceTrait;
use crate::features::tags::entity::Tag; use crate::features::tags::dto::TagWithCountDto as TagWithCount;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};
use tracing::warn;

/// Production tag service implementation
#[derive(Clone)]
pub struct TagServiceImpl {
    pub(crate) db_pool: SqlitePool,
    document_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    llm_cache: Arc<RwLock<Option<Arc<dyn LLMPort>>>>,
}

impl TagServiceImpl {
    /// Create a new tag service
    pub fn new(db_pool: SqlitePool, llm_cache: Arc<RwLock<Option<Arc<dyn LLMPort>>>>) -> Self {
        Self {
            db_pool,
            document_locks: Arc::new(Mutex::new(HashMap::new())),
            llm_cache,
        }
    }

    /// Get or create a document lock
    async fn get_document_lock(&self, document_id: &str) -> Arc<Mutex<()>> {
        let mut locks = self.document_locks.lock().await;
        locks
            .entry(document_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Merge existing tags with new tags, removing duplicates (case-insensitive)
    pub fn merge_tags_static(existing: Vec<String>, generated: Vec<String>) -> Vec<String> {
        use std::collections::HashSet;

        // Normalize to lowercase for deduplication
        let mut normalized = HashSet::new();
        let mut result = Vec::new();

        // Add existing tags first (preserving original case)
        for tag in existing {
            let normalized_tag = tag.to_lowercase();
            if normalized.insert(normalized_tag) {
                result.push(tag);
            }
        }

        // Add generated tags that aren't duplicates
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
impl TagServiceTrait for TagServiceImpl {
    async fn create_tag(&self, name: &str, color: Option<&str>) -> Result<Tag> {
        // Validate input
        if name.is_empty() || name.len() > 100 {
            return Err(AppError::InvalidInput(
                "Tag name must be 1-100 characters".into(),
            ));
        }

        let normalized_name = name.trim().to_lowercase();
        let tag_repo = TagRepository::new(self.db_pool.clone());

        tag_repo
            .create(&normalized_name, color)
            .await
            .map_err(|e| AppError::Database(format!("Failed to create tag: {}", e)))
    }

    async fn update_tag(
        &self,
        tag_id: &str,
        name: Option<&str>,
        color: Option<&str>,
    ) -> Result<Tag> {
        // Validate name if provided
        if let Some(n) = name {
            if n.is_empty() || n.len() > 100 {
                return Err(AppError::InvalidInput(
                    "Tag name must be 1-100 characters".into(),
                ));
            }
        }

        let normalized_name = name.map(|n| n.trim().to_lowercase());
        let tag_repo = TagRepository::new(self.db_pool.clone());

        tag_repo
            .update(tag_id, normalized_name.as_deref(), color)
            .await
            .map_err(|e| AppError::Database(format!("Failed to update tag: {}", e)))
    }

    async fn delete_tag(&self, tag_id: &str) -> Result<()> {
        let tag_repo = TagRepository::new(self.db_pool.clone());

        tag_repo
            .delete(tag_id)
            .await
            .map_err(|e| AppError::Database(format!("Failed to delete tag: {}", e)))
    }

    async fn get_all_tags(&self) -> Result<Vec<Tag>> {
        let tag_repo = TagRepository::new(self.db_pool.clone());

        tag_repo
            .get_all()
            .await
            .map_err(|e| AppError::Database(format!("Failed to get tags: {}", e)))
    }

    async fn get_all_tags_with_counts(&self) -> Result<Vec<TagWithCount>> {
        let tag_repo = TagRepository::new(self.db_pool.clone());

        let results = tag_repo
            .get_all_with_counts()
            .await
            .map_err(|e| AppError::Database(format!("Failed to get tags with counts: {}", e)))?;

        // Convert Vec<(Tag, i64)> to Vec<TagWithCount>
        Ok(results
            .into_iter()
            .map(|(tag, count)| TagWithCount {
                id: tag.id().to_string(),
                name: tag.name().to_string(),
                color: Some(tag.color().to_string()),
                created_at: tag.created_at().to_rfc3339(),
                document_count: count,
            })
            .collect())
    }

    async fn get_tags_for_document(&self, document_id: &str) -> Result<Vec<Tag>> {
        let tag_repo = TagRepository::new(self.db_pool.clone());

        tag_repo
            .get_tags_for_document(document_id)
            .await
            .map_err(|e| AppError::Database(format!("Failed to get document tags: {}", e)))
    }

    async fn apply_tags(&self, document_id: &str, tag_names: Vec<String>) -> Result<Vec<Tag>> {
        let tag_repo = TagRepository::new(self.db_pool.clone());

        // Get existing tags for the document
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
        let merged_tags = Self::merge_tags_static(existing_tag_names, normalized_tags);

        // Apply merged tags
        tag_repo
            .add_tags_to_document_by_names(document_id, merged_tags)
            .await
            .map_err(|e| AppError::Database(format!("Failed to apply tags: {}", e)))?;

        // Return updated tags
        tag_repo
            .get_tags_for_document(document_id)
            .await
            .map_err(|e| AppError::Database(format!("Failed to get updated tags: {}", e)))
    }

    async fn remove_tag_from_document(&self, document_id: &str, tag_id: &str) -> Result<()> {
        let tag_repo = TagRepository::new(self.db_pool.clone());

        tag_repo
            .remove_tag_from_document(document_id, tag_id)
            .await
            .map_err(|e| AppError::Database(format!("Failed to remove tag: {}", e)))
    }

    async fn search_documents_by_tag(&self, tag_name: &str) -> Result<Vec<String>> {
        let tag_repo = TagRepository::new(self.db_pool.clone());

        tag_repo
            .find_documents_by_tag_name(tag_name)
            .await
            .map_err(|e| AppError::Database(format!("Failed to search by tag: {}", e)))
    }

    async fn generate_tags(&self, document_id: &str, max_tags: usize) -> Result<Vec<String>> {
        if max_tags == 0 || max_tags > 20 {
            return Err(AppError::InvalidInput(
                "Max tags must be between 1 and 20".into(),
            ));
        }

        let _lock = self.acquire_lock_with_timeout(document_id).await?;

        let document_repo = DocumentRepositoryImpl::new(self.db_pool.clone());
        let document = document_repo
            .find_by_id(document_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Document not found".into()))?;

        let content = document.content();
        if content.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Document has no content available for tag generation".into(),
            ));
        }

        let metadata = DocumentMetadata {
            title: Some(document.file_name().to_string()),
            file_type: document.file_type().map(|t| t.to_string()),
            author: None,
        };

        let user_message = TagGenerator::build_user_message(content, &metadata, max_tags);
        let prompt = format!("{}\n\n{}", TAG_GENERATION_SYSTEM_PROMPT, user_message);
        let llm = self
            .llm_cache
            .read()
            .map_err(|e| AppError::Other(format!("LLM cache lock poisoned: {}", e)))?
            .clone()
            .ok_or_else(|| {
                AppError::AiModelsNotInstalled("Default chat model not installed".to_string())
            })?;
        let response = llm.generate(&prompt, &[], None).await?;

        if response.trim().is_empty() {
            return Ok(Vec::new());
        }

        let tags = TagGenerator::parse_tags(&response, max_tags)
            .into_iter()
            .filter(|tag| !tag.is_empty() && tag.len() <= 100)
            .collect();

        Ok(tags)
    }

    async fn auto_tag_all_documents(&self, _max_documents: usize) -> Result<usize> {
        const DEFAULT_MAX_TAGS: usize = 5;

        let max_documents = _max_documents;
        if max_documents == 0 {
            return Err(AppError::InvalidInput(
                "Max documents must be greater than 0".to_string(),
            ));
        }

        let document_repo = DocumentRepositoryImpl::new(self.db_pool.clone());
        let tag_repo = TagRepository::new(self.db_pool.clone());

        let documents = document_repo.find_all().await?;
        if documents.is_empty() {
            return Ok(0);
        }

        let document_ids: Vec<String> = documents.iter().map(|doc| doc.id().to_string()).collect();
        let tags_by_document = tag_repo.get_tags_for_documents(&document_ids).await?;

        let mut untagged_ids = Vec::new();
        for doc_id in document_ids {
            let has_tags = tags_by_document
                .get(&doc_id)
                .map(|tags| !tags.is_empty())
                .unwrap_or(false);
            if !has_tags {
                untagged_ids.push(doc_id);
            }
        }

        let mut tagged_count = 0;
        for document_id in untagged_ids.into_iter().take(max_documents) {
            let tags = match self.generate_tags(&document_id, DEFAULT_MAX_TAGS).await {
                Ok(tags) => tags,
                Err(error) => {
                    warn!(
                        "Auto-tagging failed for document {}: {}",
                        document_id, error
                    );
                    continue;
                }
            };

            if tags.is_empty() {
                continue;
            }

            if let Err(error) = self.apply_tags(&document_id, tags).await {
                warn!(
                    "Auto-tagging failed to apply tags for document {}: {}",
                    document_id, error
                );
                continue;
            }

            tagged_count += 1;
        }

        Ok(tagged_count)
    }

    async fn acquire_lock_with_timeout(&self, document_id: &str) -> Result<DocumentLockGuard> {
        let lock = self.get_document_lock(document_id).await;

        // Use lock_owned() to get an OwnedMutexGuard that owns an Arc to the mutex
        // This eliminates lifetime issues without unsafe code
        match timeout(Duration::from_secs(30), lock.lock_owned()).await {
            Ok(guard) => {
                let lock_guard = DocumentLockGuard::new(guard);
                Ok(lock_guard)
            }
            Err(_) => Err(AppError::Other(
                "Tag operation timed out - another operation is in progress for this document"
                    .to_string(),
            )),
        }
    }

    fn merge_tags(&self, existing: Vec<String>, generated: Vec<String>) -> Vec<String> {
        Self::merge_tags_static(existing, generated)
    }

    async fn acquire_lock(&self, document_id: &str) -> Result<DocumentLockGuard> {
        let lock = self.get_document_lock(document_id).await;
        // Use lock_owned() to get an OwnedMutexGuard that owns an Arc to the mutex
        let guard = lock.lock_owned().await;
        Ok(DocumentLockGuard::new(guard))
    }

    async fn get_or_create(&self, name: &str, color: &str) -> Result<Tag> {
        let tag_repo = TagRepository::new(self.db_pool.clone());
        tag_repo
            .get_or_create(name, Some(color))
            .await
            .map_err(|e| AppError::Database(format!("Failed to get or create tag: {}", e)))
    }

    async fn get_all_with_counts(&self) -> Result<Vec<TagWithCount>> {
        // Alias for get_all_tags_with_counts
        self.get_all_tags_with_counts().await
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

        let merged = TagServiceImpl::merge_tags_static(existing, generated);

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

        let merged = TagServiceImpl::merge_tags_static(existing, generated);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_merge_tags_all_duplicates() {
        let existing = vec!["tag1".to_string(), "tag2".to_string()];
        let generated = vec!["TAG1".to_string(), "Tag2".to_string()];

        let merged = TagServiceImpl::merge_tags_static(existing, generated);
        assert_eq!(merged.len(), 2);
    }
}
