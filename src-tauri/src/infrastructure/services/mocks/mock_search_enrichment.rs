//! Mock search enrichment service for testing.

use crate::infrastructure::services::traits::SearchEnrichmentServiceTrait;
use crate::shared::error::Result;
use async_trait::async_trait;
use std::sync::{Arc, RwLock};

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
                crate::features::search::enrichment_service::DocumentMetadata,
            >,
        >,
    >,
}

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
        metadata: crate::features::search::enrichment_service::DocumentMetadata,
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

impl Default for MockSearchEnrichmentService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SearchEnrichmentServiceTrait for MockSearchEnrichmentService {
    async fn enrich_results(
        &self,
        chunk_ids: &[String],
    ) -> Result<
        std::collections::HashMap<
            String,
            crate::features::search::enrichment_service::DocumentMetadata,
        >,
    > {
        let metadata_map = self.mock_metadata.read().unwrap();
        let mut result = std::collections::HashMap::new();

        for chunk_id in chunk_ids {
            if let Some(meta) = metadata_map.get(chunk_id) {
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
                    crate::features::search::enrichment_service::DocumentMetadata {
                        snippet: "Mock content snippet for testing...".to_string(),
                        content: "Mock chunk body for testing".to_string(),
                        document_id: chunk_id.clone(),
                        metadata: default_metadata,
                    },
                );
            }
        }

        Ok(result)
    }
}
