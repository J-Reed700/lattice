//! Text search port for BM25 and full-text search.
//!
//! This port defines the interface for keyword-based text search engines.
//! Infrastructure implementations can use SQLite FTS5, Tantivy, or other
//! text search libraries.

use crate::application::contracts::search::SearchResultRecord as SearchResultPortDto;
use crate::shared::error::Result;
use async_trait::async_trait;
use std::collections::HashSet;

/// Port for keyword-based text search operations.
///
/// Implementations must:
/// - Support BM25 or similar ranking algorithms
/// - Handle phrase queries, boolean operators, wildcards
/// - Perform case-insensitive matching by default
/// - Be thread-safe (`Send + Sync`)
#[async_trait]
pub trait TextSearchPort: Send + Sync {
    /// Search for documents matching a text query.
    ///
    /// Uses keyword matching with BM25 ranking. Supports:
    /// - Multi-word queries (implicit AND)
    /// - Phrase queries with quotes: "exact phrase"
    /// - Boolean operators: AND, OR, NOT
    /// - Wildcards: prefix*
    ///
    /// Results are ranked by relevance (BM25 score).
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if query is malformed (unclosed quotes, invalid operators)
    /// - `AppError::InvalidInput` if top_k is 0
    /// - `AppError::SearchFailed` if search index is unavailable or corrupted
    async fn search(&self, query: &str, top_k: usize) -> Result<Vec<SearchResultPortDto>>;

    /// Search with optional hard scope hints.
    ///
    /// - `space_id`: preferred scope key for storage-backed implementations.
    /// - `allowed_document_ids`: explicit allow-list fallback.
    ///
    /// Implementations should prefer pre-filtering in the search backend.
    /// The default implementation delegates to `search` then filters.
    async fn search_scoped(
        &self,
        query: &str,
        top_k: usize,
        space_id: Option<&str>,
        allowed_document_ids: Option<&HashSet<String>>,
    ) -> Result<Vec<SearchResultPortDto>> {
        let _ = space_id;
        let mut results = self.search(query, top_k).await?;
        if let Some(scope) = allowed_document_ids {
            results.retain(|result| scope.contains(&result.doc_id));
            if results.len() > top_k {
                results.truncate(top_k);
            }
        }
        Ok(results)
    }

    /// Index a document for text search.
    ///
    /// Adds or updates the document in the search index. If a document with
    /// the same ID already exists, it is replaced.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if content is too large for index
    /// - `AppError::SearchFailed` if indexing operation fails
    async fn index_document(&self, id: &str, content: &str) -> Result<()>;

    /// Index multiple documents in a single batch operation.
    ///
    /// More efficient than calling `index_document` repeatedly.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if batch is too large or contains invalid data
    /// - `AppError::SearchFailed` if batch indexing fails
    async fn index_batch(&self, documents: &[(&str, &str)]) -> Result<()>;

    /// Remove a document from the search index.
    ///
    /// If the ID does not exist, this is a no-op (returns Ok).
    ///
    /// # Errors
    ///
    /// - `AppError::SearchFailed` if deletion fails
    async fn remove_document(&self, id: &str) -> Result<()>;

    /// Clear all documents from the search index.
    ///
    /// This resets the index to an empty state.
    ///
    /// # Errors
    ///
    /// - `AppError::SearchFailed` if clear operation fails
    async fn clear(&self) -> Result<()>;

    /// Get the number of documents in the search index.
    async fn count(&self) -> Result<usize>;
}
