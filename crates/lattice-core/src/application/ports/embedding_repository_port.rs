//! Embedding repository port for persisting domain embeddings and raw vector data.

use crate::domain::entities::Embedding;
use crate::shared::error::Result;
use async_trait::async_trait;

/// Port for embedding persistence operations.
#[async_trait]
pub trait EmbeddingRepositoryPort: Send + Sync {
    /// Create a new embedding (convenience method for tests).
    async fn create(&self, chunk_id: &str, vector: &[f32], model: &str) -> Result<String>;

    /// Find embedding by chunk ID (convenience method for tests).
    async fn find_by_chunk(&self, chunk_id: &str) -> Result<Option<Embedding>>;

    /// Save embedding metadata and vector.
    async fn save(&self, entity: &Embedding, vector: Vec<f32>) -> Result<()>;

    /// Save multiple embeddings in a batch transaction.
    async fn save_batch(&self, entries: Vec<(Embedding, Vec<f32>)>) -> Result<()>;

    /// Find embedding metadata and vector by chunk ID.
    async fn find_by_chunk_id(&self, chunk_id: &str) -> Result<Option<(Embedding, Vec<f32>)>>;

    /// Find all embeddings for a document.
    async fn find_by_document_id(&self, document_id: &str) -> Result<Vec<(Embedding, Vec<f32>)>>;

    /// Delete embedding for a specific chunk.
    async fn delete_by_chunk_id(&self, chunk_id: &str) -> Result<()>;

    /// Delete all embeddings for a document.
    async fn delete_by_document_id(&self, document_id: &str) -> Result<()>;

    /// Count total embeddings in the system.
    async fn count(&self) -> Result<i64>;

    /// Create multiple embeddings in a batch operation.
    async fn create_batch(&self, entries: Vec<(Embedding, Vec<f32>)>) -> Result<Vec<String>>;
}
