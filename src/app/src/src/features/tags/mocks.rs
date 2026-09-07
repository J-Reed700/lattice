//! Mock implementations for testing
//!
//! This module provides mock implementations of service traits.

#[cfg(test)]
use crate::features::tags::{TagRepositoryTrait, TagServiceTrait};
#[cfg(test)]
use crate::infrastructure::services::traits::*;
#[cfg(test)]
use crate::shared::error::Result;
// Removed: use crate::shared::traits (god object eliminated - trait now in infrastructure/services/traits/)
#[cfg(test)]
pub use crate::features::tags::service::DocumentLockGuard;
#[cfg(test)]
use crate::shared::domain_types::TagName;
#[cfg(test)]
use async_trait::async_trait;
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::{Arc, Mutex, RwLock};

// ============================================================================

#[cfg(test)]
/// Mock tag service for testing
pub struct MockTagService {
    tags: std::sync::Arc<
        parking_lot::RwLock<std::collections::HashMap<String, crate::features::tags::entity::Tag>>,
    >,
    // Track which tags are applied to which documents
    document_tags: std::sync::Arc<
        parking_lot::RwLock<std::collections::HashMap<String, std::collections::HashSet<String>>>,
    >,
}

#[cfg(test)]
impl MockTagService {
    pub fn new() -> Self {
        Self {
            tags: std::sync::Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new())),
            document_tags: std::sync::Arc::new(parking_lot::RwLock::new(
                std::collections::HashMap::new(),
            )),
        }
    }
}

#[cfg(test)]
impl Default for MockTagService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
#[cfg(test)]
impl TagServiceTrait for MockTagService {
    async fn create_tag(
        &self,
        name: &str,
        color: Option<&str>,
    ) -> Result<crate::features::tags::entity::Tag> {
        let tag_name = TagName::new(name.to_lowercase())
            .map_err(|e| crate::error::AppError::InvalidData(format!("Invalid tag name: {}", e)))?;
        let tag = crate::features::tags::entity::Tag::new(
            tag_name,
            color.unwrap_or("#6366f1").to_string(),
        );
        self.tags.write().insert(name.to_lowercase(), tag.clone());
        Ok(tag)
    }

    async fn update_tag(
        &self,
        tag_id: &str,
        name: Option<&str>,
        color: Option<&str>,
    ) -> Result<crate::features::tags::entity::Tag> {
        let mut tags = self.tags.write();
        if let Some((_, tag)) = tags.iter_mut().find(|(_, t)| t.id().to_string() == tag_id) {
            if let Some(n) = name {
                let tag_name = TagName::new(n.to_lowercase()).map_err(|e| {
                    crate::error::AppError::InvalidData(format!("Invalid tag name: {}", e))
                })?;
                *tag = crate::features::tags::entity::Tag::new(
                    tag_name,
                    color.unwrap_or(tag.color()).to_string(),
                );
            }
            Ok(tag.clone())
        } else {
            Err(crate::error::AppError::NotFound("Tag not found".into()))
        }
    }

    async fn delete_tag(&self, tag_id: &str) -> Result<()> {
        self.tags
            .write()
            .retain(|_, tag| tag.id().to_string() != tag_id);
        Ok(())
    }

    async fn get_all_tags(&self) -> Result<Vec<crate::features::tags::entity::Tag>> {
        Ok(self.tags.read().values().cloned().collect())
    }

    async fn get_all_tags_with_counts(
        &self,
    ) -> Result<Vec<crate::features::tags::dto::TagWithCountDto>> {
        self.get_all_with_counts().await
    }

    async fn get_tags_for_document(
        &self,
        document_id: &str,
    ) -> Result<Vec<crate::features::tags::entity::Tag>> {
        // Step 1: Get tag names (read lock, no await)
        let tag_names: Vec<String> = {
            let document_tags = self.document_tags.read();
            document_tags
                .get(document_id)
                .map(|set| set.iter().cloned().collect())
                .unwrap_or_default()
        }; // Read lock released here

        // Step 2: Create/get tag objects (with awaits, no lock held)
        let mut tags = Vec::new();
        for name in tag_names {
            let tag = self.get_or_create(&name, "#6366f1").await?;
            tags.push(tag);
        }
        Ok(tags)
    }

    async fn apply_tags(
        &self,
        document_id: &str,
        tag_names: Vec<String>,
    ) -> Result<Vec<crate::features::tags::entity::Tag>> {
        // Step 1: Add normalized tag names to document's set (no awaits, lock held briefly)
        {
            let mut document_tags = self.document_tags.write();
            let doc_tag_set = document_tags.entry(document_id.to_string()).or_default();

            // Add normalized names only - no async calls
            for name in &tag_names {
                doc_tag_set.insert(name.to_lowercase());
            }
        } // Write lock released here

        // Step 2: Get all tag names for this document (read lock, no await)
        let tag_names_for_doc: Vec<String> = {
            let document_tags = self.document_tags.read();
            document_tags
                .get(document_id)
                .map(|set| set.iter().cloned().collect())
                .unwrap_or_default()
        }; // Read lock released here

        // Step 3: Create/get tag objects (with awaits, no locks held)
        let mut all_tags = Vec::new();
        for name in tag_names_for_doc {
            let tag = self.get_or_create(&name, "#6366f1").await?;
            all_tags.push(tag);
        }

        Ok(all_tags)
    }

    async fn remove_tag_from_document(&self, document_id: &str, tag_id: &str) -> Result<()> {
        // Find the tag name by ID
        let tag_name_opt: Option<String> = {
            let tags = self.tags.read();
            tags.values()
                .find(|t| t.id().to_string() == tag_id)
                .map(|t| t.name().as_str().to_lowercase())
        }; // Lock released

        if let Some(tag_name) = tag_name_opt {
            // Remove the tag name from the document's tag set
            let mut document_tags = self.document_tags.write();
            if let Some(doc_tag_set) = document_tags.get_mut(document_id) {
                doc_tag_set.remove(&tag_name);
            }
        }

        Ok(())
    }

    async fn search_documents_by_tag(&self, tag_name: &str) -> Result<Vec<String>> {
        let normalized = tag_name.to_lowercase();

        // Search through document_tags to find documents with this tag
        let document_ids: Vec<String> = {
            let document_tags = self.document_tags.read();
            document_tags
                .iter()
                .filter(|(_, tag_set)| tag_set.contains(&normalized))
                .map(|(doc_id, _)| doc_id.clone())
                .collect()
        }; // Lock released

        Ok(document_ids)
    }

    async fn generate_tags(&self, _document_id: &str, max_tags: usize) -> Result<Vec<String>> {
        // Mock: return sample generated tags
        let sample_tags = vec![
            "rust".to_string(),
            "programming".to_string(),
            "tutorial".to_string(),
            "documentation".to_string(),
            "guide".to_string(),
        ];
        Ok(sample_tags.into_iter().take(max_tags).collect())
    }

    async fn auto_tag_all_documents(&self, _max_documents: usize) -> Result<usize> {
        // Mock: return 0 since auto-tagging is disabled
        Ok(0)
    }

    async fn acquire_lock_with_timeout(&self, _document_id: &str) -> Result<DocumentLockGuard> {
        Ok(DocumentLockGuard::mock())
    }

    async fn acquire_lock(&self, _document_id: &str) -> Result<DocumentLockGuard> {
        Ok(DocumentLockGuard::mock())
    }

    fn merge_tags(&self, mut existing: Vec<String>, generated: Vec<String>) -> Vec<String> {
        use std::collections::HashSet;

        let mut seen = HashSet::new();
        for tag in &existing {
            seen.insert(tag.to_lowercase());
        }

        for tag in generated {
            if seen.insert(tag.to_lowercase()) {
                existing.push(tag.to_lowercase());
            }
        }

        existing
    }

    async fn get_or_create(
        &self,
        name: &str,
        color: &str,
    ) -> Result<crate::features::tags::entity::Tag> {
        let normalized = name.to_lowercase();
        let mut tags = self.tags.write();

        if let Some(tag) = tags.get(&normalized) {
            Ok(tag.clone())
        } else {
            // Use normalized (lowercase) name to ensure consistency
            let tag_name = TagName::new(normalized.clone()).map_err(|e| {
                crate::error::AppError::InvalidData(format!("Invalid tag name: {}", e))
            })?;
            let tag = crate::features::tags::entity::Tag::new(tag_name, color.to_string());
            tags.insert(normalized, tag.clone());
            Ok(tag)
        }
    }

    async fn get_all_with_counts(
        &self,
    ) -> Result<Vec<crate::features::tags::dto::TagWithCountDto>> {
        let tags = self.tags.read();
        Ok(tags
            .values()
            .map(|tag| crate::features::tags::dto::TagWithCountDto {
                id: tag.id().to_string(),
                name: tag.name().to_string(),
                color: Some(tag.color().to_string()),
                created_at: "2024-01-01T00:00:00Z".to_string(),
                document_count: 0,
            })
            .collect())
    }
}

#[cfg(test)]
/// Mock implementation of TagRepositoryTrait for testing
///
/// Stores tag entities in-memory for deterministic testing without database dependencies.
pub struct MockTagRepository {
    tags: Arc<RwLock<std::collections::HashMap<String, crate::features::tags::entity::Tag>>>,
    name_index: Arc<RwLock<std::collections::HashMap<String, String>>>, // lowercase_name -> id
    document_tags: Arc<RwLock<std::collections::HashMap<String, Vec<String>>>>, // document_id -> tag_ids
}

#[cfg(test)]
impl MockTagRepository {
    /// Create a new mock tag repository
    pub fn new() -> Self {
        Self {
            tags: Arc::new(RwLock::new(std::collections::HashMap::new())),
            name_index: Arc::new(RwLock::new(std::collections::HashMap::new())),
            document_tags: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Add a tag entity to the mock repository
    ///
    /// # Example
    /// ```rust
    /// use lattice::domain::entities::tag::Tag;
    /// use lattice::domain_types::TagName;
    /// use lattice::services::traits::MockTagRepository;
    ///
    /// let mock = MockTagRepository::new();
    /// let name = TagName::new("rust".to_string()).unwrap();
    /// let tag = Tag::new(name, "#ff5733".to_string());
    /// mock.add_tag(tag);
    /// ```
    pub fn add_tag(&self, tag: crate::features::tags::entity::Tag) {
        let id = tag.id().as_str().to_string();
        let name_key = tag.name().as_str().to_lowercase();

        self.tags.write().unwrap().insert(id.clone(), tag);
        self.name_index.write().unwrap().insert(name_key, id);
    }

    /// Clear all tags
    pub fn clear(&self) {
        self.tags.write().unwrap().clear();
        self.name_index.write().unwrap().clear();
        self.document_tags.write().unwrap().clear();
    }

    /// Get tag count
    pub fn len(&self) -> usize {
        self.tags.read().unwrap().len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.tags.read().unwrap().is_empty()
    }
}

#[cfg(test)]
impl Default for MockTagRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
#[cfg(test)]
impl TagRepositoryTrait for MockTagRepository {
    async fn create_tag(
        &self,
        name: &str,
        color: Option<&str>,
    ) -> Result<crate::features::tags::entity::Tag> {
        let entity = self.get_or_create(name, color).await?;
        Ok(entity)
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<crate::features::tags::entity::Tag>> {
        let name_key = name.to_lowercase();
        let name_index = self.name_index.read().unwrap();

        if let Some(id) = name_index.get(&name_key) {
            let tags = self.tags.read().unwrap();
            Ok(tags.get(id).cloned())
        } else {
            Ok(None)
        }
    }

    async fn get_or_create(
        &self,
        name: &str,
        color: Option<&str>,
    ) -> Result<crate::features::tags::entity::Tag> {
        // Try to find existing tag
        if let Some(tag) = self.find_by_name(name).await? {
            return Ok(tag);
        }

        // Create new tag
        let tag_name = crate::domain_types::TagName::new(name.to_string())
            .map_err(|e| crate::error::AppError::InvalidData(format!("Invalid tag name: {}", e)))?;
        let tag_color = color.unwrap_or("#6366f1");
        let tag = crate::features::tags::entity::Tag::new(tag_name, tag_color.to_string());

        // Save to mock storage
        let id = tag.id().as_str().to_string();
        let name_key = tag.name().as_str().to_lowercase();

        self.tags.write().unwrap().insert(id.clone(), tag.clone());
        self.name_index.write().unwrap().insert(name_key, id);

        Ok(tag)
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<crate::features::tags::entity::Tag>> {
        Ok(self.tags.read().unwrap().get(id).cloned())
    }

    async fn find_by_filter(
        &self,
        _filter: &dyn crate::application::ports::Filter,
    ) -> Result<Vec<crate::features::tags::entity::Tag>> {
        // For mock, just return all tags
        self.find_all().await
    }

    async fn find_all(&self) -> Result<Vec<crate::features::tags::entity::Tag>> {
        let mut tags: Vec<_> = self.tags.read().unwrap().values().cloned().collect();
        // Sort by name
        tags.sort_by(|a, b| a.name().as_str().cmp(b.name().as_str()));
        Ok(tags)
    }

    async fn save(&self, entity: &crate::features::tags::entity::Tag) -> Result<()> {
        let id = entity.id().as_str().to_string();
        let name_key = entity.name().as_str().to_lowercase();

        self.tags
            .write()
            .unwrap()
            .insert(id.clone(), entity.clone());
        self.name_index.write().unwrap().insert(name_key, id);

        Ok(())
    }

    async fn save_batch(&self, entities: &[crate::features::tags::entity::Tag]) -> Result<()> {
        for entity in entities {
            self.save(entity).await?;
        }
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        if let Some(tag) = self.tags.write().unwrap().remove(id) {
            let name_key = tag.name().as_str().to_lowercase();
            self.name_index.write().unwrap().remove(&name_key);
        }
        Ok(())
    }

    async fn delete_batch(&self, ids: &[&str]) -> Result<()> {
        for id in ids {
            self.delete(id).await?;
        }
        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        Ok(self.tags.read().unwrap().len())
    }

    async fn exists(&self, id: &str) -> Result<bool> {
        Ok(self.tags.read().unwrap().contains_key(id))
    }

    async fn get_tags_for_document(
        &self,
        document_id: &str,
    ) -> Result<Vec<crate::features::tags::entity::Tag>> {
        let document_tags = self.document_tags.read().unwrap();
        let tag_ids = document_tags.get(document_id).cloned().unwrap_or_default();

        let tags_lock = self.tags.read().unwrap();
        let mut tags = Vec::new();
        for tag_id in tag_ids {
            if let Some(tag) = tags_lock.get(&tag_id) {
                tags.push(tag.clone());
            }
        }
        Ok(tags)
    }

    async fn add_tag_to_document(&self, document_id: &str, tag_id: &str) -> Result<()> {
        let mut document_tags = self.document_tags.write().unwrap();
        let tags = document_tags.entry(document_id.to_string()).or_default();
        if !tags.contains(&tag_id.to_string()) {
            tags.push(tag_id.to_string());
        }
        Ok(())
    }

    async fn remove_tag_from_document(&self, document_id: &str, tag_id: &str) -> Result<()> {
        let mut document_tags = self.document_tags.write().unwrap();
        if let Some(tags) = document_tags.get_mut(document_id) {
            tags.retain(|id| id != tag_id);
        }
        Ok(())
    }

    async fn find_documents_by_tag(&self, tag_id: &str) -> Result<Vec<String>> {
        let document_tags = self.document_tags.read().unwrap();
        let mut docs = Vec::new();
        for (doc_id, tag_ids) in document_tags.iter() {
            if tag_ids.contains(&tag_id.to_string()) {
                docs.push(doc_id.clone());
            }
        }
        Ok(docs)
    }

    async fn find_documents_by_tag_name(&self, _tag_name: &str) -> Result<Vec<String>> {
        // Mock: return empty list
        Ok(Vec::new())
    }

    async fn get_all_with_counts(&self) -> Result<Vec<(crate::features::tags::entity::Tag, i64)>> {
        let tags = self.tags.read().unwrap();
        let document_tags = self.document_tags.read().unwrap();

        // Count how many documents have each tag
        let mut tag_counts: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();
        for tag_ids in document_tags.values() {
            for tag_id in tag_ids {
                *tag_counts.entry(tag_id.clone()).or_insert(0) += 1;
            }
        }

        Ok(tags
            .values()
            .map(|tag| {
                let count = *tag_counts.get(tag.id().as_str()).unwrap_or(&0);
                (tag.clone(), count)
            })
            .collect())
    }

    async fn update(
        &self,
        tag_id: &str,
        name: Option<&str>,
        color: Option<&str>,
    ) -> Result<crate::features::tags::entity::Tag> {
        let mut tags = self.tags.write().unwrap();
        let tag = tags
            .get_mut(tag_id)
            .ok_or_else(|| crate::shared::error::AppError::NotFound("Tag not found".to_string()))?;

        // Mock: create new tag with updated values
        let updated_name = match name {
            Some(n) => crate::domain_types::TagName::new(n.to_string()).map_err(|e| {
                crate::shared::error::AppError::InvalidData(format!("Invalid tag name: {}", e))
            })?,
            None => tag.name().clone(),
        };
        let updated_color = color.unwrap_or(tag.color()).to_string();
        let updated_tag = crate::features::tags::entity::Tag::new(updated_name, updated_color);

        *tag = updated_tag.clone();
        Ok(updated_tag)
    }

    async fn add_tags_to_document_by_names(
        &self,
        _document_id: &str,
        tag_names: Vec<String>,
    ) -> Result<Vec<crate::features::tags::entity::Tag>> {
        // Mock: get or create tags
        let mut tags = Vec::new();
        for name in tag_names {
            let tag = self.get_or_create(&name, None).await?;
            tags.push(tag);
        }
        Ok(tags)
    }

    async fn get_tags_for_documents(
        &self,
        _document_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<crate::features::tags::entity::Tag>>> {
        // Mock: return empty map
        Ok(std::collections::HashMap::new())
    }

    async fn remove_all_tags_from_document(&self, _document_id: &str) -> Result<()> {
        // Mock: no-op
        Ok(())
    }

    async fn get_or_create_batch(
        &self,
        tag_names: Vec<String>,
    ) -> Result<Vec<crate::features::tags::entity::Tag>> {
        let mut tags = Vec::new();
        for name in tag_names {
            let tag = self.get_or_create(&name, None).await?;
            tags.push(tag);
        }
        Ok(tags)
    }

    async fn add_tags_to_documents_batch(
        &self,
        _document_tags: Vec<(String, Vec<String>)>,
    ) -> Result<usize> {
        // Mock: return count of document-tag pairs
        Ok(0)
    }
}

// ============================================================================
// Function Calling Traits
// ============================================================================

// Phase 8.1: Commented out unused import
// use crate::features::function_calling::dto::*;
use crate::features::function_calling::domain::*;
