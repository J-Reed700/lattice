//! In-memory `MockMentionRepository` over `MentionRepositoryPort`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use parking_lot::RwLock;

use lattice_core::shared::error::{AppError, Result};

use super::Mention;

/// In-memory mock implementation of MentionRepositoryTrait (LEGACY - migrated to DDD)
#[derive(Clone)]
pub struct MockMentionRepository {
    mentions: Arc<RwLock<HashMap<String, Mention>>>,
    name_index: Arc<RwLock<HashMap<String, String>>>, // name -> id
    mention_documents: Arc<RwLock<HashMap<String, Vec<String>>>>, // mention_id -> document_ids
}

impl MockMentionRepository {
    pub fn new() -> Self {
        Self {
            mentions: Arc::new(RwLock::new(HashMap::new())),
            name_index: Arc::new(RwLock::new(HashMap::new())),
            mention_documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn clear(&self) {
        self.mentions.write().clear();
        self.name_index.write().clear();
        self.mention_documents.write().clear();
    }
}

impl Default for MockMentionRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl lattice_core::application::ports::MentionRepositoryPort for MockMentionRepository {
    async fn create_mention(
        &self,
        name: &str,
        mention_type: &str,
        metadata: Option<&str>,
    ) -> Result<lattice_core::application::ports::MentionData, AppError> {
        use lattice_core::shared::types::{ChunkId, DocumentId};
        use uuid::Uuid;

        // For the mock, we need to create a simplified representation
        // Real Mention entity requires document_id and chunk_id which we don't have here
        // So we'll just store the data we need for the port without using the full entity

        let id_str = Uuid::new_v4().to_string();
        let created_at = Utc::now().to_rfc3339();

        if let Some(existing_id) = self.name_index.read().get(name).cloned() {
            if let Some(_mention) = self.mentions.read().get(&existing_id) {
                return Ok(lattice_core::application::ports::MentionData {
                    id: existing_id,
                    name: name.to_string(),
                    mention_type: mention_type.to_string(),
                    metadata: metadata.map(|s| s.to_string()),
                    created_at,
                });
            }
        }

        let mention = Mention::new(
            DocumentId::new(), // Dummy document ID
            ChunkId::new(),    // Dummy chunk ID
            name.to_string(),
            mention_type.parse().unwrap_or_default(),
            0,
            name.len(),
        );

        self.mentions.write().insert(id_str.clone(), mention);
        self.name_index
            .write()
            .insert(name.to_string(), id_str.clone());

        Ok(lattice_core::application::ports::MentionData {
            id: id_str,
            name: name.to_string(),
            mention_type: mention_type.to_string(),
            metadata: metadata.map(|s| s.to_string()),
            created_at,
        })
    }

    async fn find_mention_by_name(
        &self,
        name: &str,
    ) -> Result<Option<lattice_core::application::ports::MentionData>, AppError> {
        let name_index = self.name_index.read();
        if let Some(id) = name_index.get(name) {
            if let Some(mention) = self.mentions.read().get(id).cloned() {
                return Ok(Some(lattice_core::application::ports::MentionData {
                    id: id.clone(),
                    name: mention.text().to_string(),
                    mention_type: mention.mention_type().to_string(),
                    metadata: None,
                    created_at: mention.created_at().to_rfc3339(),
                }));
            }
        }
        Ok(None)
    }

    async fn search_mentions(
        &self,
        query: &str,
        limit: i64,
    ) -> Result<Vec<lattice_core::application::ports::MentionData>, AppError> {
        let mentions = self.mentions.read();
        let name_index = self.name_index.read();

        let mut id_to_name: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for (name, id) in name_index.iter() {
            id_to_name.insert(id.clone(), name.clone());
        }

        let mut results: Vec<lattice_core::application::ports::MentionData> = mentions
            .iter()
            .filter(|(_, m)| m.text().contains(query))
            .map(|(id, m)| lattice_core::application::ports::MentionData {
                id: id.clone(),
                name: id_to_name
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| m.text().to_string()),
                mention_type: m.mention_type().to_string(),
                metadata: None,
                created_at: m.created_at().to_rfc3339(),
            })
            .collect();
        results.sort_by(|a, b| a.name.cmp(&b.name));
        results.truncate(limit as usize);
        Ok(results)
    }

    async fn get_mentions_by_type(
        &self,
        mention_type: &str,
    ) -> Result<Vec<lattice_core::application::ports::MentionData>, AppError> {
        let mentions = self.mentions.read();
        let name_index = self.name_index.read();

        let mut id_to_name: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for (name, id) in name_index.iter() {
            id_to_name.insert(id.clone(), name.clone());
        }

        let mut results: Vec<lattice_core::application::ports::MentionData> = mentions
            .iter()
            .filter(|(_, m)| m.mention_type().to_string() == mention_type)
            .map(|(id, m)| lattice_core::application::ports::MentionData {
                id: id.clone(),
                name: id_to_name
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| m.text().to_string()),
                mention_type: m.mention_type().to_string(),
                metadata: None,
                created_at: m.created_at().to_rfc3339(),
            })
            .collect();
        results.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(results)
    }

    async fn get_mentions_for_document(
        &self,
        _document_id: &str,
    ) -> Result<Vec<lattice_core::application::ports::MentionWithContextData>, AppError> {
        Ok(vec![])
    }

    async fn get_documents_with_mention(&self, mention_id: &str) -> Result<Vec<String>, AppError> {
        Ok(self
            .mention_documents
            .read()
            .get(mention_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn extract_and_store_mentions(
        &self,
        _document_id: &str,
        _text: &str,
    ) -> Result<Vec<lattice_core::application::ports::MentionWithContextData>, AppError> {
        Ok(vec![])
    }

    async fn update_mention(
        &self,
        id: &str,
        mention_type: Option<&str>,
        metadata: Option<&str>,
    ) -> Result<lattice_core::application::ports::MentionData, AppError> {
        let mentions = self.mentions.read();
        let mention = mentions
            .get(id)
            .ok_or_else(|| AppError::NotFound(format!("Mention not found: {}", id)))?;

        let name_index = self.name_index.read();
        let name = name_index
            .iter()
            .find(|(_, mid)| *mid == id)
            .map(|(n, _)| n.clone())
            .unwrap_or_else(|| mention.text().to_string());

        // Create updated data - convert to owned string to avoid lifetime issues
        let current_type = mention.mention_type().to_string();
        let new_type = mention_type.unwrap_or(&current_type);
        let created_at = mention.created_at().to_rfc3339();

        Ok(lattice_core::application::ports::MentionData {
            id: id.to_string(),
            name,
            mention_type: new_type.to_string(),
            metadata: metadata.map(|s| s.to_string()),
            created_at,
        })
    }

    async fn delete_mention(&self, id: &str) -> Result<(), AppError> {
        if let Some(_mention) = self.mentions.write().remove(id) {
            let mut name_to_remove = None;
            for (name, mention_id) in self.name_index.read().iter() {
                if mention_id == id {
                    name_to_remove = Some(name.clone());
                    break;
                }
            }
            if let Some(name) = name_to_remove {
                self.name_index.write().remove(&name);
            }
            self.mention_documents.write().remove(id);
        }
        Ok(())
    }
}

// Legacy MentionRepositoryTrait implementation - MIGRATED TO DDD
// This implementation has been replaced with MentionRepositoryPort.
// See src/infrastructure/services/mocks/mock_mention.rs for the new DDD implementation.
// See src/infrastructure/services/traits/mention.rs for migration guide.
// Migration:
// - OLD: Arc<dyn MentionRepositoryTrait>
// - NEW: Arc<dyn MentionRepositoryPort>
// #[async_trait]
// impl MentionRepositoryTrait for MockMentionRepository {
//     async fn create_mention(
//         &self,
//         name: &str,
//         mention_type: &str,
//         metadata: Option<&str>,
//     ) -> Result<Mention> {
//         // Check if exists, update if so
//         if let Some(existing_id) = self.name_index.read().get(name).cloned() {
//             let mut mentions = self.mentions.write();
//             if let Some(mention) = mentions.get_mut(&existing_id) {
//                 mention.mention_type = mention_type.to_string();
//                 mention.metadata = metadata.map(|s| s.to_string());
//                 return Ok(mention.clone());
//             }
//         }
//         let id = Uuid::new_v4().to_string();
//         let created_at = Utc::now().to_rfc3339();
//         let mention = Mention {
//             id: id.clone(),
//             name: name.to_string(),
//             mention_type: mention_type.to_string(),
//             metadata: metadata.map(|s| s.to_string()),
//             created_at,
//         };
//         self.mentions.write().insert(id.clone(), mention.clone());
//         self.name_index.write().insert(name.to_string(), id);
//         Ok(mention)
//     }
//     async fn find_mention_by_name(&self, name: &str) -> Result<Option<Mention>> {
//         let name_index = self.name_index.read();
//         if let Some(id) = name_index.get(name) {
//             Ok(self.mentions.read().get(id).cloned())
//         } else {
//             Ok(None)
//         }
//     }
//     async fn search_mentions(&self, query: &str, limit: i64) -> Result<Vec<Mention>> {
//         let mentions = self.mentions.read();
//         let mut results: Vec<Mention> = mentions
//             .values()
//             .filter(|m| m.name.contains(query))
//             .cloned()
//             .collect();
//         results.sort_by(|a, b| a.name.cmp(&b.name));
//         results.truncate(limit as usize);
//         Ok(results)
//     }
//     async fn get_mentions_by_type(&self, mention_type: &str) -> Result<Vec<Mention>> {
//         let mentions = self.mentions.read();
//         let mut results: Vec<Mention> = mentions
//             .values()
//             .filter(|m| m.mention_type == mention_type)
//             .cloned()
//             .collect();
//         results.sort_by(|a, b| a.name.cmp(&b.name));
//         Ok(results)
//     }
//     async fn link_mention_to_document(
//         &self,
//         document_id: &str,
//         mention_id: &str,
//         context: Option<&str>,
//         position: Option<i64>,
//     ) -> Result<DocumentMention> {
//         let id = Uuid::new_v4().to_string();
//         let created_at = Utc::now().to_rfc3339();
//         let doc_mention = DocumentMention {
//             id,
//             document_id: document_id.to_string(),
//             mention_id: mention_id.to_string(),
//             context: context.map(|s| s.to_string()),
//             position,
//             created_at,
//         };
//         self.document_mentions
//             .write()
//             .entry(document_id.to_string())
//             .or_insert_with(Vec::new)
//             .push(doc_mention.clone());
//         self.mention_documents
//             .write()
//             .entry(mention_id.to_string())
//             .or_insert_with(Vec::new)
//             .push(document_id.to_string());
//         Ok(doc_mention)
//     }
//     async fn get_mentions_for_document(
//         &self,
//         document_id: &str,
//     ) -> Result<Vec<MentionWithContext>> {
//         let document_mentions = self.document_mentions.read();
//         if let Some(doc_mentions) = document_mentions.get(document_id) {
//             let mentions_map = self.mentions.read();
//             let mut results: Vec<MentionWithContext> = doc_mentions
//                 .iter()
//                 .filter_map(|dm| {
//                     mentions_map
//                         .get(&dm.mention_id)
//                         .map(|m| MentionWithContext {
//                             mention: m.clone(),
//                             context: dm.context.clone(),
//                             position: dm.position,
//                         })
//                 })
//                 .collect();
//             results.sort_by(|a, b| a.position.cmp(&b.position));
//             Ok(results)
//         } else {
//             Ok(Vec::new())
//         }
//     }
//     async fn get_documents_with_mention(&self, mention_id: &str) -> Result<Vec<String>> {
//         Ok(self
//             .mention_documents
//             .read()
//             .get(mention_id)
//             .cloned()
//             .unwrap_or_default())
//     }
//     async fn clear_document_mentions(&self, document_id: &str) -> Result<()> {
//         if let Some(doc_mentions) = self.document_mentions.write().remove(document_id) {
//             let mut mention_documents = self.mention_documents.write();
//             for dm in doc_mentions {
//                 if let Some(docs) = mention_documents.get_mut(&dm.mention_id) {
//                     docs.retain(|id| id != document_id);
//                 }
//             }
//         }
//         Ok(())
//     }
//     async fn delete_mention(&self, id: &str) -> Result<()> {
//         if let Some(mention) = self.mentions.write().remove(id) {
//             self.name_index.write().remove(&mention.name);
//             if let Some(doc_ids) = self.mention_documents.write().remove(id) {
//                 let mut document_mentions = self.document_mentions.write();
//                 for doc_id in doc_ids {
//                     if let Some(mentions) = document_mentions.get_mut(&doc_id) {
//                         mentions.retain(|dm| dm.mention_id != id);
//                     }
//                 }
//             }
//         }
//         Ok(())
//     }
//     fn extract_mentions_from_text(&self, text: &str) -> HashMap<String, Vec<(String, usize)>> {
//         use once_cell::sync::Lazy;
//         use regex::Regex;
//         static AT_MENTION_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"@\[([^\]]+)\]").unwrap());
//         static WIKILINK_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[\[([^\]]+)\]\]").unwrap());
//         let mut mentions: HashMap<String, Vec<(String, usize)>> = HashMap::new();
//         for cap in AT_MENTION_RE.captures_iter(text) {
//             let name = cap[1].to_string();
//             let position = cap.get(0).unwrap().start();
//             mentions
//                 .entry("person".to_string())
//                 .or_insert_with(Vec::new)
//                 .push((name, position));
//         }
//         for cap in WIKILINK_RE.captures_iter(text) {
//             let name = cap[1].to_string();
//             let position = cap.get(0).unwrap().start();
//             mentions
//                 .entry("wikilink".to_string())
//                 .or_insert_with(Vec::new)
//                 .push((name, position));
//         }
//         mentions
//     }
//     async fn extract_and_store_mentions(
//         &self,
//         document_id: &str,
//         text: &str,
//     ) -> Result<Vec<MentionWithContext>> {
//         self.clear_document_mentions(document_id).await?;
//         let extracted = self.extract_mentions_from_text(text);
//         let mut results = Vec::new();
//         for (mention_type, occurrences) in extracted {
//             for (name, position) in occurrences {
//                 let mention = self.create_mention(&name, &mention_type, None).await?;
//                 let start = position.saturating_sub(50);
//                 let end = (position + 50).min(text.len());
//                 let context = text[start..end].to_string();
//                 self.link_mention_to_document(
//                     document_id,
//                     &mention.id,
//                     Some(&context),
//                     Some(position as i64),
//                 )
//                 .await?;
//                 results.push(MentionWithContext {
//                     mention,
//                     context: Some(context),
//                     position: Some(position as i64),
//                 });
//             }
//         }
//         Ok(results)
//     }
// }
