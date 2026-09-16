//! Chunk repository port for chunk-specific queries.
//!
//! This port extends the generic RepositoryPort with chunk-specific operations.
//! While the generic repository handles basic CRUD, this port provides chunk
//! domain operations like document-based lookups and cascade deletions.
//!
//! # Purpose
//!
//! - Provides chunk-specific repository operations
//! - Supports finding chunks by document ID
//! - Enables cascade deletion of chunks when documents are removed
//! - Maintains chunk-document relationships
//!
//! # Infrastructure Implementations
//!
//! - `ChunkRepository` - SQLite implementation
//!
//! # Example Usage
//!
//! ```rust
//! use crate::application::ports::ChunkRepositoryPort;
//!
//! async fn delete_document_chunks(
//!     repo: &dyn ChunkRepositoryPort,
//!     doc_id: &str,
//! ) -> Result<()> {
//!     repo.delete_by_document(doc_id).await
//! }
//! ```

use crate::application::ports::RepositoryPort;
use crate::domain::entities::chunk::Chunk;
use crate::shared::error::Result;
use async_trait::async_trait;

/// Port for chunk-specific repository operations.
///
/// Provides operations for working with chunk-document relationships
/// and chunk lifecycle management.
#[async_trait]
pub trait ChunkRepositoryPort: RepositoryPort<Chunk> + Send + Sync {
    /// Create a new chunk (convenience method for tests).
    ///
    /// # Arguments
    ///
    /// * `document_id` - The document ID
    /// * `content` - The chunk content
    /// * `context_prefix` - Optional context prefix
    /// * `contextualized_content` - Optional contextualized content
    /// * `index` - Chunk index within document
    /// * `start_char` - Optional start character offset
    /// * `end_char` - Optional end character offset
    ///
    /// # Returns
    ///
    /// The created chunk entity.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if creation fails
    #[allow(clippy::too_many_arguments)]
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
    ///
    /// # Arguments
    ///
    /// * `document_id` - The unique document identifier
    ///
    /// # Returns
    ///
    /// Vector of chunks belonging to the document (may be empty).
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    /// - `AppError::InvalidInput` if document ID is invalid
    ///
    /// # Example
    ///
    /// ```rust
    /// let chunks = repo.find_by_document("doc-123").await?;
    /// println!("Found {} chunks", chunks.len());
    /// ```
    async fn find_by_document(&self, document_id: &str) -> Result<Vec<Chunk>>;

    /// Find chunks by a set of IDs (batch lookup).
    ///
    /// # Arguments
    ///
    /// * `chunk_ids` - Chunk IDs to lookup
    ///
    /// # Returns
    ///
    /// Vector of chunks found (missing IDs are ignored).
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    async fn find_by_ids(&self, chunk_ids: &[String]) -> Result<Vec<Chunk>>;

    /// Delete all chunks for a given document.
    ///
    /// This is typically used during document deletion to cascade
    /// the removal to associated chunks.
    ///
    /// # Arguments
    ///
    /// * `document_id` - The unique document identifier
    ///
    /// # Returns
    ///
    /// Ok(()) if deletion succeeds (even if no chunks were found).
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if deletion fails
    /// - `AppError::InvalidInput` if document ID is invalid
    ///
    /// # Example
    ///
    /// ```rust
    /// // Delete all chunks for a document
    /// repo.delete_by_document("doc-123").await?;
    /// ```
    async fn delete_by_document(&self, document_id: &str) -> Result<()>;

    /// Count all chunks in the repository.
    ///
    /// # Returns
    ///
    /// Total count of all chunks across all documents.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if count query fails
    async fn count_all(&self) -> Result<i64>;

    /// Count the number of indexed documents (documents that have at least one chunk).
    ///
    /// # Returns
    ///
    /// Total count of unique documents that have chunks.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if count query fails
    async fn count_indexed_documents(&self) -> Result<i64>;

    /// Count chunks for a specific document.
    ///
    /// # Arguments
    ///
    /// * `document_id` - The document ID to count chunks for
    ///
    /// # Returns
    ///
    /// Number of chunks belonging to the document.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if count query fails
    async fn count_by_document(&self, document_id: &str) -> Result<i64>;

    /// Create multiple chunks in a batch operation.
    ///
    /// More efficient than multiple `create()` calls.
    ///
    /// # Arguments
    ///
    /// * `chunks` - Vector of chunk data to create
    ///
    /// # Returns
    ///
    /// Vector of created chunks.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if batch creation fails
    async fn create_batch(&self, chunks: Vec<Chunk>) -> Result<Vec<Chunk>>;
}
