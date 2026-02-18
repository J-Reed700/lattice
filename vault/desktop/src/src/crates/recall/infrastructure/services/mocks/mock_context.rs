//! Mock implementations for testing
//!
//! This module provides mock implementations of service traits.

#[cfg(test)]
use crate::infrastructure::search::service::SearchResult;
#[cfg(test)]
use crate::infrastructure::services::traits::*;
#[cfg(test)]
use crate::shared::error::Result;
#[cfg(test)]
use async_trait::async_trait;
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::{Arc, Mutex, RwLock};

#[cfg(test)]
/// Mock context manager for testing
///
/// Simulates context management without actual token counting complexity.
/// Allows testing of context formatting and composition.
/// All operations are deterministic and fast.
pub struct MockContextManager {
    max_context_tokens: usize,
}

#[cfg(test)]
impl MockContextManager {
    /// Create new mock with default token budget
    pub fn new() -> Self {
        Self {
            max_context_tokens: 4000,
        }
    }

    /// Create mock with custom token budget
    pub fn with_budget(max_context_tokens: usize) -> Self {
        Self { max_context_tokens }
    }
}

#[cfg(test)]
impl Default for MockContextManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl ContextManagerTrait for MockContextManager {
    fn build_context_for_llm(
        &self,
        conversation: &crate::domain::conversation::ConversationAggregate,
        search_results: Vec<SearchResult>,
    ) -> Result<crate::services::context_manager::LLMContext> {
        // Simple mock implementation
        let system_prompt = self.format_system_context(conversation.system_prompt());
        let messages = self.format_conversation_history(conversation.messages());
        let document_context = self.format_document_context(search_results, 2000)?;

        // Simplified token counting
        let total_tokens = crate::infrastructure::qa::tokenizer::count_tokens(&system_prompt)
            + messages
                .iter()
                .map(|m| crate::infrastructure::qa::tokenizer::count_tokens(&m.content))
                .sum::<usize>()
            + crate::infrastructure::qa::tokenizer::count_tokens(&document_context);

        Ok(crate::services::context_manager::LLMContext {
            system_prompt,
            messages,
            document_context,
            total_tokens,
        })
    }

    fn format_system_context(&self, system_prompt: Option<&str>) -> String {
        crate::services::context_manager::ContextManager::format_system_context(system_prompt)
    }

    fn format_document_context(
        &self,
        search_results: Vec<SearchResult>,
        max_tokens: usize,
    ) -> Result<String> {
        crate::services::context_manager::ContextManager::format_document_context(
            search_results,
            max_tokens,
        )
    }

    fn format_conversation_history(
        &self,
        messages: &[crate::domain::conversation::ConversationMessage],
    ) -> Vec<crate::domain::conversation::LLMMessage> {
        crate::services::context_manager::ContextManager::format_conversation_history(messages)
    }

    fn max_context_tokens(&self) -> usize {
        self.max_context_tokens
    }
}

#[cfg(test)]
/// Mock search enrichment service for testing
///
/// Simulates search result enrichment without database queries.
/// Allows configuration of metadata and tracking of enrichment requests.
/// All operations are deterministic and thread-safe.
pub struct MockSearchEnrichmentService {
    mock_metadata: Arc<
        RwLock<
            std::collections::HashMap<
                String,
                crate::services::search_enrichment_service::DocumentMetadata,
            >,
        >,
    >,
}

#[cfg(test)]
impl MockSearchEnrichmentService {
    /// Create new mock service with empty metadata
    pub fn new() -> Self {
        Self {
            mock_metadata: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Set metadata for a chunk ID
    ///
    /// When `enrich_results` is called with this chunk ID, the configured metadata will be returned.
    ///
    /// # Arguments
    /// * `chunk_id` - The chunk ID to match
    /// * `metadata` - The DocumentMetadata to return
    ///
    /// # Example
    /// ```rust
    /// let service = MockSearchEnrichmentService::new();
    /// service.set_metadata(
    ///     "chunk1",
    ///     DocumentMetadata {
    ///         snippet: "Custom snippet".to_string(),
    ///         metadata: HashMap::from([
    ///             ("filename".to_string(), json!("custom.txt")),
    ///             ("file_type".to_string(), json!("text/plain")),
    ///         ]),
    ///     }
    /// );
    /// ```
    pub fn set_metadata(
        &self,
        chunk_id: &str,
        metadata: crate::services::search_enrichment_service::DocumentMetadata,
    ) {
        self.mock_metadata
            .write()
            .unwrap()
            .insert(chunk_id.to_string(), metadata);
    }

    /// Clear all configured metadata
    ///
    /// Resets the mock to initial empty state.
    pub fn clear(&self) {
        self.mock_metadata.write().unwrap().clear();
    }
}

#[cfg(test)]
impl Default for MockSearchEnrichmentService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
#[cfg(test)]
impl SearchEnrichmentServiceTrait for MockSearchEnrichmentService {
    async fn enrich_results(
        &self,
        chunk_ids: &[String],
    ) -> Result<
        std::collections::HashMap<
            String,
            crate::services::search_enrichment_service::DocumentMetadata,
        >,
    > {
        let metadata_map = self.mock_metadata.read().unwrap();
        let mut result = std::collections::HashMap::new();

        for chunk_id in chunk_ids {
            if let Some(meta) = metadata_map.get(chunk_id) {
                // Return configured metadata
                result.insert(chunk_id.clone(), meta.clone());
            } else {
                // Default metadata
                let mut default_metadata = std::collections::HashMap::new();
                default_metadata.insert(
                    "filename".to_string(),
                    serde_json::Value::String("mock_document.txt".to_string()),
                );
                default_metadata.insert(
                    "file_type".to_string(),
                    serde_json::Value::String("text/plain".to_string()),
                );
                default_metadata.insert(
                    "file_size".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(1024)),
                );
                default_metadata.insert(
                    "created_at".to_string(),
                    serde_json::Value::String("2024-01-01T00:00:00Z".to_string()),
                );
                default_metadata.insert(
                    "updated_at".to_string(),
                    serde_json::Value::String("2024-01-01T00:00:00Z".to_string()),
                );

                result.insert(
                    chunk_id.clone(),
                    crate::services::search_enrichment_service::DocumentMetadata {
                        snippet: "Mock content snippet for testing...".to_string(),
                        document_id: chunk_id.clone(),
                        metadata: default_metadata,
                    },
                );
            }
        }

        Ok(result)
    }
}
