//! Text search port for BM25 and full-text search.
//!
//! This port defines the interface for keyword-based text search engines.
//! Infrastructure implementations can use SQLite FTS5, Tantivy, or other
//! text search libraries.
//!
//! # Purpose
//!
//! - Abstracts keyword search implementation details
//! - Complements vector search with traditional text matching
//! - Enables hybrid search combining semantic and keyword matching
//! - Supports testing with mock search results
//!
//! # Infrastructure Implementations
//!
//! - `SqliteFtsAdapter` - SQLite FTS5 full-text search
//! - `TantivyAdapter` - Tantivy inverted index search
//! - `MockTextSearchAdapter` - Test implementation returning fixed results
//!
//! # Example Usage
//!
//! ```rust
//! use crate::application::ports::TextSearchPort;
//! use crate::features::search::dto::SearchResultPortDto;
//!
//! async fn search_documents(
//!     searcher: &impl TextSearchPort,
//!     query: &str,
//! ) -> Result<Vec<SearchResultPortDto>> {
//!     searcher.search(query, 20).await
//! }
//! ```

use crate::features::search::dto::SearchResultPortDto;
use crate::shared::result::Result;
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
    /// # Arguments
    ///
    /// * `query` - The search query string
    /// * `top_k` - Maximum number of results to return
    ///
    /// # Returns
    ///
    /// A vector of `SearchResultPortDto` objects ordered by relevance (highest first).
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if query is malformed (unclosed quotes, invalid operators)
    /// - `AppError::InvalidInput` if top_k is 0
    /// - `AppError::SearchFailed` if search index is unavailable or corrupted
    ///
    /// # Example
    ///
    /// ```rust
    /// // Simple query
    /// let results = searcher.search("machine learning", 10).await?;
    ///
    /// // Phrase query
    /// let results = searcher.search("\"neural network\"", 10).await?;
    ///
    /// // Boolean query
    /// let results = searcher.search("rust AND async", 10).await?;
    /// ```
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
    /// # Arguments
    ///
    /// * `id` - Unique identifier for the document (e.g., "doc-123", "chunk-456")
    /// * `content` - The text content to index
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if content is too large for index
    /// - `AppError::SearchFailed` if indexing operation fails
    ///
    /// # Example
    ///
    /// ```rust
    /// searcher.index_document(
    ///     "doc-123",
    ///     "This is the document content to be indexed",
    /// ).await?;
    /// ```
    async fn index_document(&self, id: &str, content: &str) -> Result<()>;

    /// Index multiple documents in a single batch operation.
    ///
    /// More efficient than calling `index_document` repeatedly.
    ///
    /// # Arguments
    ///
    /// * `documents` - Slice of (id, content) tuples to index
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if batch is too large or contains invalid data
    /// - `AppError::SearchFailed` if batch indexing fails
    ///
    /// # Example
    ///
    /// ```rust
    /// let docs = vec![
    ///     ("doc-1", "First document"),
    ///     ("doc-2", "Second document"),
    /// ];
    /// searcher.index_batch(&docs).await?;
    /// ```
    async fn index_batch(&self, documents: &[(&str, &str)]) -> Result<()>;

    /// Remove a document from the search index.
    ///
    /// If the ID does not exist, this is a no-op (returns Ok).
    ///
    /// # Arguments
    ///
    /// * `id` - The unique identifier of the document to remove
    ///
    /// # Errors
    ///
    /// - `AppError::SearchFailed` if deletion fails
    ///
    /// # Example
    ///
    /// ```rust
    /// searcher.remove_document("doc-123").await?;
    /// ```
    async fn remove_document(&self, id: &str) -> Result<()>;

    /// Clear all documents from the search index.
    ///
    /// This resets the index to an empty state.
    ///
    /// # Errors
    ///
    /// - `AppError::SearchFailed` if clear operation fails
    ///
    /// # Example
    ///
    /// ```rust
    /// searcher.clear().await?;
    /// ```
    async fn clear(&self) -> Result<()>;

    /// Get the number of documents in the search index.
    ///
    /// # Returns
    ///
    /// The total number of indexed documents.
    ///
    /// # Example
    ///
    /// ```rust
    /// let count = searcher.count().await?;
    /// println!("Index contains {} documents", count);
    /// ```
    async fn count(&self) -> Result<usize>;
}
