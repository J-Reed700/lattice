//! Embedding Repository Port
//!
//! Specialized port for embedding persistence that handles the dual-struct pattern.
//! Unlike generic `RepositoryPort<T>`, this explicitly manages vector data separately
//! from domain metadata.

use crate::domain::entities::Embedding;
use crate::shared::result::Result;
use async_trait::async_trait;

/// Port for embedding persistence operations.
///
/// # Architecture
///
/// Embeddings have a unique requirement: they contain both business metadata
/// (domain concern) and vector data (infrastructure concern for ML).
///
/// This port handles both:
/// - **Entity**: Domain `Embedding` with metadata
/// - **Vector**: Raw `Vec<f32>` for ML operations
///
/// # Example
///
/// ```rust
/// use crate::application::ports::EmbeddingRepositoryPort;
///
/// async fn store_embedding(
///     repo: &dyn EmbeddingRepositoryPort,
///     entity: Embedding,
///     vector: Vec<f32>
/// ) -> Result<()> {
///     repo.save(&entity, vector).await
/// }
/// ```
#[async_trait]
pub trait EmbeddingRepositoryPort: Send + Sync {
    /// Create a new embedding (convenience method for tests)
    ///
    /// # Arguments
    /// * `chunk_id` - Chunk ID
    /// * `vector` - Embedding vector data
    /// * `model` - Model name used for embedding
    ///
    /// # Returns
    /// The embedding ID
    async fn create(&self, chunk_id: &str, vector: &[f32], model: &str) -> Result<String>;

    /// Find embedding by chunk ID (convenience method for tests)
    ///
    /// # Arguments
    /// * `chunk_id` - Chunk ID
    ///
    /// # Returns
    /// Optional embedding with metadata
    async fn find_by_chunk(&self, chunk_id: &str) -> Result<Option<Embedding>>;

    /// Save embedding metadata and vector
    ///
    /// # Arguments
    /// * `entity` - Domain embedding (metadata)
    /// * `vector` - Embedding vector data
    async fn save(&self, entity: &Embedding, vector: Vec<f32>) -> Result<()>;

    /// Save multiple embeddings in a batch transaction
    ///
    /// More efficient than multiple `save()` calls.
    ///
    /// # Arguments
    /// * `entries` - Vec of (entity, vector) pairs
    async fn save_batch(&self, entries: Vec<(Embedding, Vec<f32>)>) -> Result<()>;

    /// Find embedding by chunk ID
    ///
    /// Returns both entity (metadata) and vector if found.
    ///
    /// # Returns
    /// `Option<(Embedding, Vec<f32>)>` - Entity and vector, or None
    async fn find_by_chunk_id(&self, chunk_id: &str) -> Result<Option<(Embedding, Vec<f32>)>>;

    /// Find all embeddings for a document
    ///
    /// Returns embeddings for all chunks belonging to the document.
    ///
    /// # Arguments
    /// * `document_id` - Document ID
    ///
    /// # Returns
    /// Vec of (entity, vector) pairs
    async fn find_by_document_id(&self, document_id: &str) -> Result<Vec<(Embedding, Vec<f32>)>>;

    /// Delete embedding for a specific chunk
    async fn delete_by_chunk_id(&self, chunk_id: &str) -> Result<()>;

    /// Delete all embeddings for a document
    async fn delete_by_document_id(&self, document_id: &str) -> Result<()>;

    /// Count total embeddings in the system
    async fn count(&self) -> Result<i64>;

    /// Create multiple embeddings in a batch operation.
    ///
    /// More efficient than multiple `save()` calls.
    ///
    /// # Arguments
    ///
    /// * `entries` - Vector of (entity, vector) pairs to create
    ///
    /// # Returns
    ///
    /// Vector of created embedding IDs.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if batch creation fails
    async fn create_batch(&self, entries: Vec<(Embedding, Vec<f32>)>) -> Result<Vec<String>>;
}
