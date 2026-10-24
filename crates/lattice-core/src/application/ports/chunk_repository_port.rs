//! Chunk repository port for chunk-specific queries and lifecycle management.

use crate::application::ports::RepositoryPort;
use crate::domain::entities::chunk::Chunk;
use crate::shared::error::Result;
use async_trait::async_trait;
use std::collections::HashMap;

/// Port for chunk-specific repository operations.
#[async_trait]
pub trait ChunkRepositoryPort: RepositoryPort<Chunk> + Send + Sync {
    /// Create a new chunk (convenience method for tests).
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

    /// Find all chunks for a given document.
    async fn find_by_document(&self, document_id: &str) -> Result<Vec<Chunk>>;

    /// The first `limit` chunks of a document, in chunk order.
    async fn find_first_by_document(&self, document_id: &str, limit: usize) -> Result<Vec<Chunk>> {
        let mut chunks = self.find_by_document(document_id).await?;
        chunks.sort_by_key(|chunk| chunk.index());
        chunks.truncate(limit);
        Ok(chunks)
    }

    /// Find chunks by a set of IDs (batch lookup).
    async fn find_by_ids(&self, chunk_ids: &[String]) -> Result<Vec<Chunk>>;

    /// Return only content for the requested candidate IDs without retaining corpus text in index.
    async fn find_content_by_ids(&self, chunk_ids: &[String]) -> Result<HashMap<String, String>> {
        let rows = self.find_by_ids(chunk_ids).await?;
        Ok(rows
            .into_iter()
            .map(|chunk| (chunk.id().to_string(), chunk.content().to_string()))
            .collect())
    }

    /// IDs belonging to documents attached to a conversation (excluded from vault-wide search).
    async fn find_conversation_attached_chunk_ids(&self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    /// Delete all chunks for a given document.
    async fn delete_by_document(&self, document_id: &str) -> Result<()>;

    /// Count all chunks in the repository.
    async fn count_all(&self) -> Result<i64>;

    /// Count the number of indexed documents (documents with at least one chunk).
    async fn count_indexed_documents(&self) -> Result<i64>;

    /// Count chunks for a specific document.
    async fn count_by_document(&self, document_id: &str) -> Result<i64>;

    /// Create multiple chunks in a batch operation.
    async fn create_batch(&self, chunks: Vec<Chunk>) -> Result<Vec<Chunk>>;
}
