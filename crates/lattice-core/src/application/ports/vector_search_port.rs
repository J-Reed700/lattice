//! Vector similarity search port for nearest-neighbor retrieval.

use crate::application::contracts::search::SearchResultRecord as SearchResultPortDto;
use crate::shared::error::Result;
use std::collections::HashSet;

/// A persisted vector and the source metadata needed by runtime search.
pub struct VectorIndexEntry {
    pub id: String,
    pub embedding: Vec<f32>,
    pub content: String,
    pub chunk_id: String,
    pub document_id: String,
}

/// Port for vector similarity search operations.
pub trait VectorSearchPort: Send + Sync {
    /// Search for the most similar vectors to a query embedding.
    fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        threshold: f32,
    ) -> Result<Vec<SearchResultPortDto>>;

    /// Search restricted to an explicit set of documents.
    ///
    /// `allowed_document_ids` is a hard scope: `None` searches everything, and
    /// `Some(set)` returns only chunks whose `doc_id` is in `set` (an empty set
    /// allows nothing). At most `top_k` results come back, ordered by descending
    /// similarity, exactly as for [`VectorSearchPort::search`].
    ///
    /// There is deliberately no default body. Post-filtering an unscoped
    /// `search` silently loses recall — an ANN index has to widen its candidate
    /// window *before* retrieval to still return `top_k` in-scope hits — so
    /// implementations must apply the scope during retrieval where they can,
    /// and must opt into filtering after the fact where they cannot.
    fn search_scoped(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        threshold: f32,
        allowed_document_ids: Option<&HashSet<String>>,
    ) -> Result<Vec<SearchResultPortDto>>;

    /// Add a new embedding to the search index.
    fn add_embedding(&self, id: String, embedding: Vec<f32>) -> Result<()>;

    /// Add a new embedding with associated content metadata to the search index.
    fn add_embedding_with_content(
        &self,
        id: String,
        embedding: Vec<f32>,
        content: String,
        chunk_id: String,
        document_id: String,
    ) -> Result<()> {
        let _ = (content, chunk_id, document_id);
        self.add_embedding(id, embedding)
    }

    /// Publish a document's persisted vectors in one batch. Implementations
    /// should persist once and accept repeated publication of the same IDs so
    /// an interrupted import can finish without generating embeddings again.
    fn publish_embeddings(&self, entries: Vec<VectorIndexEntry>) -> Result<()> {
        for entry in entries {
            self.add_embedding_with_content(
                entry.id,
                entry.embedding,
                entry.content,
                entry.chunk_id,
                entry.document_id,
            )?;
        }
        Ok(())
    }

    /// Remove an embedding from the search index.
    fn remove_embedding(&self, id: &str) -> Result<()>;

    /// Remove a set of embeddings, persisting once when supported.
    fn remove_embeddings(&self, ids: &[String]) -> Result<()> {
        for id in ids {
            self.remove_embedding(id)?;
        }
        Ok(())
    }

    /// Clear all embeddings from the index.
    fn clear(&self) -> Result<()>;

    /// Get the number of embeddings in the index.
    fn count(&self) -> usize;

    /// Get the expected dimension of embeddings for this index.
    fn dimension(&self) -> usize;
}
