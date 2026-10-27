//! Chunk repository port: chunk-specific queries, such as lookups by document
//! and cascade deletion, on top of the generic `RepositoryPort`.

use crate::application::ports::RepositoryPort;
use crate::domain::entities::chunk::Chunk;
use crate::shared::error::Result;
use async_trait::async_trait;
use std::collections::HashMap;

/// Port for chunk-specific repository operations.
///
/// Provides operations for working with chunk-document relationships
/// and chunk lifecycle management.
#[async_trait]
pub trait ChunkRepositoryPort: RepositoryPort<Chunk> + Send + Sync {
    /// Creates a chunk (convenience method for tests).
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if creation fails
    async fn create(
        &self,
        document_id: &str,
        content: &str,
        context_prefix: Option<&str>,
        contextualized_content: Option<&str>,
        index: usize,
        start_char: Option<i64>,
        end_char: Option<i64>,
    ) -> Result<Chunk>;

    /// All chunks of a document.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    /// - `AppError::InvalidInput` if document ID is invalid
    async fn find_by_document(&self, document_id: &str) -> Result<Vec<Chunk>>;

    /// The first `limit` chunks of a document, in chunk order. For callers
    /// that want a document's opening passages without reading all of it.
    async fn find_first_by_document(&self, document_id: &str, limit: usize) -> Result<Vec<Chunk>> {
        let mut chunks = self.find_by_document(document_id).await?;
        chunks.sort_by_key(|chunk| chunk.index());
        chunks.truncate(limit);
        Ok(chunks)
    }

    /// The chunks with these ids, in one lookup.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    async fn find_by_ids(&self, chunk_ids: &[String]) -> Result<Vec<Chunk>>;

    /// Return only content for the requested candidate IDs. Search callers use
    /// this after vector ranking so the index never retains the corpus text.
    async fn find_content_by_ids(&self, chunk_ids: &[String]) -> Result<HashMap<String, String>> {
        let rows = self.find_by_ids(chunk_ids).await?;
        Ok(rows
            .into_iter()
            .map(|chunk| (chunk.id().to_string(), chunk.content().to_string()))
            .collect())
    }

    /// IDs belonging to documents attached to a conversation. These chunks
    /// are excluded from vault-wide hybrid search.
    async fn find_conversation_attached_chunk_ids(&self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    /// Deletes every chunk of a document, as part of deleting the document.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if deletion fails
    /// - `AppError::InvalidInput` if document ID is invalid
    async fn delete_by_document(&self, document_id: &str) -> Result<()>;

    /// How many chunks there are across all documents.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if count query fails
    async fn count_all(&self) -> Result<i64>;

    /// How many documents have at least one chunk.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if count query fails
    async fn count_indexed_documents(&self) -> Result<i64>;

    /// How many chunks a document has.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if count query fails
    async fn count_by_document(&self, document_id: &str) -> Result<i64>;

    /// Creates several chunks in one batch operation.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if batch creation fails
    async fn create_batch(&self, chunks: Vec<Chunk>) -> Result<Vec<Chunk>>;
}
