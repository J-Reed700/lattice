//! Search Enrichment Service
//!
//! Enriches search results with document metadata.

use crate::features::search::trait_def::SearchEnrichmentServiceTrait;
use crate::infrastructure::persistence::database::connection::query_with_timeout;
use async_trait::async_trait;
use futures::stream::{self, StreamExt};
use lattice_core::shared::error::AppError;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;

/// Service for enriching search results with document metadata.
pub struct SearchEnrichmentService {
    db_pool: SqlitePool,
}

/// Document metadata for a chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// Content snippet (max 200 chars), for display.
    pub snippet: String,
    /// The chunk body as indexed.
    ///
    /// Kept beside the snippet because a cross-encoder handed a 200-character
    /// window is not reranking the passage the retriever matched — it is
    /// reranking a preview of it, which is how a reranker comes to disagree
    /// with its own first stage.
    pub content: String,
    /// Document ID
    pub document_id: String,
    /// Metadata fields
    pub metadata: HashMap<String, serde_json::Value>,
}

impl SearchEnrichmentService {
    /// Create a new search enrichment service.
    pub fn new(db_pool: SqlitePool) -> Self {
        Self { db_pool }
    }

    /// Enrich search results with document metadata using parallel batch processing.
    pub async fn enrich_results(
        &self,
        chunk_ids: &[String],
    ) -> Result<HashMap<String, DocumentMetadata>, AppError> {
        if chunk_ids.is_empty() {
            return Ok(HashMap::new());
        }

        tracing::debug!(
            chunk_count = chunk_ids.len(),
            "Starting search result enrichment"
        );

        // Split into batches of 900 (SQLite parameter limit is 999)
        let batches: Vec<Vec<String>> = chunk_ids.chunks(900).map(|chunk| chunk.to_vec()).collect();

        tracing::debug!(
            batch_count = batches.len(),
            batch_size = 900,
            "Split chunk IDs into batches"
        );

        let results = stream::iter(batches)
            .map(|batch| {
                let pool = self.db_pool.clone();
                async move { self.fetch_metadata_batch(&pool, batch).await }
            })
            .buffer_unordered(4)
            .collect::<Vec<_>>()
            .await;

        // Merge results from all batches
        let mut enriched_data = HashMap::new();
        for result in results {
            enriched_data.extend(result?);
        }

        tracing::debug!(
            enriched_count = enriched_data.len(),
            "Search result enrichment complete"
        );

        Ok(enriched_data)
    }

    /// Fetch metadata for a batch of chunk IDs (max 900).
    async fn fetch_metadata_batch(
        &self,
        pool: &SqlitePool,
        batch: Vec<String>,
    ) -> Result<HashMap<String, DocumentMetadata>, AppError> {
        let placeholders = batch.iter().map(|_| "?").collect::<Vec<_>>().join(", ");

        let query = format!(
            r#"
            SELECT
                tc.id as chunk_id,
                tc.content,
                tc.document_id,
                tc.chunk_index,
                tc.start_char,
                tc.end_char,
                d.file_name,
                d.file_path,
                d.file_type,
                d.size_bytes,
                d.created_at,
                d.updated_at
            FROM text_chunks tc
            JOIN documents d ON tc.document_id = d.id
            WHERE tc.id IN ({})
              -- A file attached to a chat is not part of the vault. Its chunks
              -- are indexed so that chat can read and cite it, and every
              -- vault-wide search command resolves its hits through this one
              -- statement, so refusing it a document here is what keeps it off
              -- the search page. The callers drop a hit they cannot resolve.
              AND d.owner_conversation_id IS NULL
            "#,
            placeholders
        );

        let mut query_builder = sqlx::query(&query);
        for id in &batch {
            query_builder = query_builder.bind(id);
        }

        let rows: Vec<sqlx::sqlite::SqliteRow> =
            query_with_timeout(|| async { query_builder.fetch_all(pool).await }).await?;

        let mut batch_data = HashMap::new();
        for row in rows {
            let chunk_id: String = row.get("chunk_id");
            let content: String = row.get("content");
            let document_id: String = row.get("document_id");
            let chunk_index: i64 = row.get("chunk_index");
            let start_char: Option<i64> = row.try_get("start_char").ok();
            let end_char: Option<i64> = row.try_get("end_char").ok();

            let snippet = snippet_of(&content);

            let mut metadata = HashMap::new();
            metadata.insert(
                "filename".to_string(),
                serde_json::Value::String(row.get("file_name")),
            );

            let file_path: String = row.get("file_path");
            metadata.insert("path".to_string(), serde_json::Value::String(file_path));

            if let Some(file_type) = row.get::<Option<String>, _>("file_type") {
                metadata.insert(
                    "file_type".to_string(),
                    serde_json::Value::String(file_type),
                );
            }

            metadata.insert(
                "file_size".to_string(),
                serde_json::Value::Number(serde_json::Number::from(
                    row.get::<i64, _>("size_bytes"),
                )),
            );

            metadata.insert(
                "created_at".to_string(),
                serde_json::Value::String(row.get("created_at")),
            );

            metadata.insert(
                "updated_at".to_string(),
                serde_json::Value::String(row.get("updated_at")),
            );
            metadata.insert(
                "chunk_index".to_string(),
                serde_json::Value::Number(serde_json::Number::from(chunk_index)),
            );
            if let Some(start) = start_char {
                metadata.insert(
                    "start_char".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(start)),
                );
            }
            if let Some(end) = end_char {
                metadata.insert(
                    "end_char".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(end)),
                );
            }

            batch_data.insert(
                chunk_id,
                DocumentMetadata {
                    snippet,
                    content,
                    document_id,
                    metadata,
                },
            );
        }

        Ok(batch_data)
    }
}

#[async_trait]
impl SearchEnrichmentServiceTrait for SearchEnrichmentService {
    async fn enrich_results(
        &self,
        chunk_ids: &[String],
    ) -> Result<
        HashMap<String, crate::features::search::enrichment_service::DocumentMetadata>,
        AppError,
    > {
        // The trait uses the same DocumentMetadata type we define
        SearchEnrichmentService::enrich_results(self, chunk_ids).await
    }
}

/// Leading portion of a chunk shown in search results, cut on a character
/// boundary. Byte-indexed slicing panics when the cut lands inside a
/// multibyte character, which any non-ASCII document will eventually do.
fn snippet_of(content: &str) -> String {
    const SNIPPET_CHARS: usize = 200;
    if content.chars().count() > SNIPPET_CHARS {
        format!(
            "{}...",
            lattice_core::shared::text::safe_truncate(content, SNIPPET_CHARS)
        )
    } else {
        content.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that empty chunk IDs return empty result
    #[tokio::test]
    async fn test_enrich_empty_chunks() {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        let service = SearchEnrichmentService::new(pool);

        let result = service.enrich_results(&[]).await.unwrap();
        assert!(result.is_empty());
    }

    /// Test snippet truncation
    #[test]
    fn test_snippet_truncation() {
        let long_text = "a".repeat(300);
        let snippet = snippet_of(&long_text);
        assert_eq!(snippet.len(), 203); // 200 chars + "..."
        assert!(snippet.ends_with("..."));

        assert_eq!(snippet_of("short"), "short");
    }

    /// A multibyte character straddling the cut must not panic; the snippet
    /// is measured in characters, never bytes.
    #[test]
    fn test_snippet_truncation_is_char_safe() {
        let japanese = "時".repeat(150); // 450 bytes, 150 chars
        let snippet = snippet_of(&japanese);
        assert_eq!(snippet, japanese);

        let long_japanese = "時".repeat(250);
        let snippet = snippet_of(&long_japanese);
        assert!(snippet.ends_with("..."));
        assert_eq!(snippet.chars().count(), 203);
        assert!(snippet.starts_with(&"時".repeat(200)));
    }

    /// Test batch size calculation
    #[test]
    fn test_batch_calculation() {
        let chunk_ids: Vec<String> = (0..2500).map(|i| format!("chunk{}", i)).collect();
        let batches: Vec<Vec<String>> = chunk_ids.chunks(900).map(|chunk| chunk.to_vec()).collect();

        assert_eq!(batches.len(), 3); // 900 + 900 + 700
        assert_eq!(batches[0].len(), 900);
        assert_eq!(batches[1].len(), 900);
        assert_eq!(batches[2].len(), 700);
    }
}
