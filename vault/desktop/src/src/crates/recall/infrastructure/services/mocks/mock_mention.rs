//! Mock implementations for testing
//!
//! This module provides mock implementations of service traits.

#[cfg(test)]
use crate::shared::error::Result;
#[cfg(test)]
use async_trait::async_trait;
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::{Arc, Mutex, RwLock};

#[cfg(test)]
/// Mock implementation of MentionRepositoryTrait for testing
///
/// Provides an in-memory implementation for unit tests without database dependencies.
pub struct MockMentionRepository {
    mentions: Arc<
        RwLock<
            std::collections::HashMap<
                String,
                crate::application::ports::mention_repository_port::MentionData,
            >,
        >,
    >,
    by_name: Arc<RwLock<std::collections::HashMap<String, String>>>, // name -> id
    by_document: Arc<
        RwLock<
            std::collections::HashMap<
                String,
                Vec<crate::application::ports::mention_repository_port::MentionWithContextData>,
            >,
        >,
    >, // document_id -> mentions
    document_mentions: Arc<RwLock<std::collections::HashMap<String, Vec<String>>>>, // mention_id -> document_ids
}

#[cfg(test)]
impl MockMentionRepository {
    /// Create a new mock repository
    pub fn new() -> Self {
        Self {
            mentions: Arc::new(RwLock::new(std::collections::HashMap::new())),
            by_name: Arc::new(RwLock::new(std::collections::HashMap::new())),
            by_document: Arc::new(RwLock::new(std::collections::HashMap::new())),
            document_mentions: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Add a mention to the mock repository
    ///
    /// # Example
    /// ```rust
    /// let mock = MockMentionRepository::new();
    /// mock.add_mention(MentionData {
    ///     id: "mention1".into(),
    ///     name: "John Doe".into(),
    ///     mention_type: "person".into(),
    ///     metadata: None,
    ///     created_at: "2024-01-01T00:00:00Z".into(),
    /// });
    /// ```
    pub fn add_mention(
        &self,
        mention: crate::application::ports::mention_repository_port::MentionData,
    ) {
        let id = mention.id.clone();
        let name = mention.name.clone();
        self.mentions.write().unwrap().insert(id.clone(), mention);
        self.by_name.write().unwrap().insert(name, id);
    }

    /// Add a mention with context to a document
    pub fn add_mention_to_document(
        &self,
        document_id: &str,
        mention: crate::application::ports::mention_repository_port::MentionWithContextData,
    ) {
        let mention_id = mention.mention.id.clone();

        // Add mention if not exists
        if !self.mentions.read().unwrap().contains_key(&mention_id) {
            self.add_mention(mention.mention.clone());
        }

        // Add to document mentions
        self.by_document
            .write()
            .unwrap()
            .entry(document_id.to_string())
            .or_default()
            .push(mention);

        // Add backlink
        self.document_mentions
            .write()
            .unwrap()
            .entry(mention_id)
            .or_default()
            .push(document_id.to_string());
    }

    /// Clear all mentions
    pub fn clear(&self) {
        self.mentions.write().unwrap().clear();
        self.by_name.write().unwrap().clear();
        self.by_document.write().unwrap().clear();
        self.document_mentions.write().unwrap().clear();
    }

    /// Get mention count
    pub fn len(&self) -> usize {
        self.mentions.read().unwrap().len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.mentions.read().unwrap().is_empty()
    }
}

#[cfg(test)]
impl Default for MockMentionRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
#[cfg(test)]
impl crate::application::ports::mention_repository_port::MentionRepositoryPort
    for MockMentionRepository
{
    async fn create_mention(
        &self,
        name: &str,
        mention_type: &str,
        metadata: Option<&str>,
    ) -> Result<crate::application::ports::mention_repository_port::MentionData> {
        // Check if mention already exists by name
        // IMPORTANT: never hold a read lock while taking a write lock on the same RwLock.
        // Clone data out of lock scopes first to avoid deadlock when updating existing mentions.
        let existing_id = self.by_name.read().unwrap().get(name).cloned();
        if let Some(existing_id) = existing_id {
            let existing = self.mentions.read().unwrap().get(&existing_id).cloned();
            if let Some(existing) = existing {
                // Update existing mention
                let updated = crate::application::ports::mention_repository_port::MentionData {
                    id: existing_id,
                    name: name.to_string(),
                    mention_type: mention_type.to_string(),
                    metadata: metadata.map(|s| s.to_string()),
                    created_at: existing.created_at,
                };
                self.mentions
                    .write()
                    .unwrap()
                    .insert(updated.id.clone(), updated.clone());
                return Ok(updated);
            }
        }

        // Create new mention
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();

        let mention = crate::application::ports::mention_repository_port::MentionData {
            id: id.clone(),
            name: name.to_string(),
            mention_type: mention_type.to_string(),
            metadata: metadata.map(|s| s.to_string()),
            created_at,
        };

        self.mentions
            .write()
            .unwrap()
            .insert(id.clone(), mention.clone());
        self.by_name.write().unwrap().insert(name.to_string(), id);

        Ok(mention)
    }

    async fn find_mention_by_name(
        &self,
        name: &str,
    ) -> Result<Option<crate::application::ports::mention_repository_port::MentionData>> {
        if let Some(id) = self.by_name.read().unwrap().get(name) {
            Ok(self.mentions.read().unwrap().get(id).cloned())
        } else {
            Ok(None)
        }
    }

    async fn search_mentions(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<crate::application::ports::mention_repository_port::MentionData>> {
        let query_lower = query.to_lowercase();
        let mentions: Vec<_> = self
            .mentions
            .read()
            .unwrap()
            .values()
            .filter(|m| m.name.to_lowercase().contains(&query_lower))
            .take(limit as usize)
            .cloned()
            .collect();

        Ok(mentions)
    }

    async fn get_mentions_by_type(
        &self,
        mention_type: &str,
    ) -> Result<Vec<crate::application::ports::mention_repository_port::MentionData>> {
        let mentions: Vec<_> = self
            .mentions
            .read()
            .unwrap()
            .values()
            .filter(|m| m.mention_type == mention_type)
            .cloned()
            .collect();

        Ok(mentions)
    }

    async fn get_mentions_for_document(
        &self,
        document_id: &str,
    ) -> Result<Vec<crate::application::ports::mention_repository_port::MentionWithContextData>>
    {
        Ok(self
            .by_document
            .read()
            .unwrap()
            .get(document_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn get_documents_with_mention(&self, mention_id: &str) -> Result<Vec<String>> {
        Ok(self
            .document_mentions
            .read()
            .unwrap()
            .get(mention_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn extract_and_store_mentions(
        &self,
        document_id: &str,
        text: &str,
    ) -> Result<Vec<crate::application::ports::mention_repository_port::MentionWithContextData>>
    {
        use regex::Regex;

        // Clear existing mentions for this document
        self.by_document.write().unwrap().remove(document_id);

        let mut results = Vec::new();

        // Extract @[person] mentions
        let at_mention_re = Regex::new(r"@\[([^\]]+)\]").unwrap();
        for cap in at_mention_re.captures_iter(text) {
            let name = &cap[1];
            let position = cap.get(0).unwrap().start();
            let context = Self::extract_context_static(text, position);

            let mention = self.create_mention(name, "person", None).await?;

            let mention_with_context =
                crate::application::ports::mention_repository_port::MentionWithContextData {
                    mention,
                    document_id: document_id.to_string(),
                    context: Some(context),
                    position: Some(position as i64),
                };

            self.add_mention_to_document(document_id, mention_with_context.clone());
            results.push(mention_with_context);
        }

        // Extract [[wikilink]] mentions
        let wikilink_re = Regex::new(r"\[\[([^\]]+)\]\]").unwrap();
        for cap in wikilink_re.captures_iter(text) {
            let name = &cap[1];
            let position = cap.get(0).unwrap().start();
            let context = Self::extract_context_static(text, position);

            let mention = self.create_mention(name, "wikilink", None).await?;

            let mention_with_context =
                crate::application::ports::mention_repository_port::MentionWithContextData {
                    mention,
                    document_id: document_id.to_string(),
                    context: Some(context),
                    position: Some(position as i64),
                };

            self.add_mention_to_document(document_id, mention_with_context.clone());
            results.push(mention_with_context);
        }

        Ok(results)
    }

    async fn update_mention(
        &self,
        id: &str,
        mention_type: Option<&str>,
        metadata: Option<&str>,
    ) -> Result<crate::application::ports::mention_repository_port::MentionData> {
        let mut mentions = self.mentions.write().unwrap();
        let mention = mentions.get_mut(id).ok_or_else(|| {
            crate::shared::error::AppError::NotFound(format!("Mention not found: {}", id))
        })?;

        // Update fields if provided
        if let Some(mt) = mention_type {
            mention.mention_type = mt.to_string();
        }
        if let Some(md) = metadata {
            mention.metadata = Some(md.to_string());
        }

        Ok(mention.clone())
    }

    async fn delete_mention(&self, id: &str) -> Result<()> {
        if let Some(mention) = self.mentions.write().unwrap().remove(id) {
            self.by_name.write().unwrap().remove(&mention.name);
            self.document_mentions.write().unwrap().remove(id);
        }
        Ok(())
    }
}

#[cfg(test)]
impl MockMentionRepository {
    /// Extract context around a mention position
    fn extract_context_static(text: &str, position: usize) -> String {
        let start = position.saturating_sub(50);
        let end = (position + 50).min(text.len());
        text[start..end].to_string()
    }
}

// ============================================================================
// Tag Repository Trait (DDD Architecture)
// ============================================================================
