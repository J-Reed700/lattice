//! Text search port for BM25 and full-text keyword search.

use crate::application::contracts::search::SearchResultRecord as SearchResultPortDto;
use crate::shared::error::Result;
use async_trait::async_trait;
use std::collections::HashSet;

/// Port for keyword-based text search operations.
#[async_trait]
pub trait TextSearchPort: Send + Sync {
    /// Search for documents matching a text query, ordered by relevance.
    async fn search(&self, query: &str, top_k: usize) -> Result<Vec<SearchResultPortDto>>;

    /// Search with optional hard scope hints (space_id or allowed document IDs).
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
    async fn index_document(&self, id: &str, content: &str) -> Result<()>;

    /// Index multiple documents in a single batch operation.
    async fn index_batch(&self, documents: &[(&str, &str)]) -> Result<()>;

    /// Remove a document from the search index.
    async fn remove_document(&self, id: &str) -> Result<()>;

    /// Clear all documents from the search index.
    async fn clear(&self) -> Result<()>;

    /// Get the number of documents in the search index.
    async fn count(&self) -> Result<usize>;
}
