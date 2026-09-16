//! Repository trait definitions for dependency injection
//!
//! This module defines the trait interfaces for all repository types in the system.
//! These traits enable dependency injection, testing with mocks, and loose coupling
//! between layers of the application.
//!
//! # Architecture
//!
//! Shared repository contracts:
//! - **Studs (Public Interface)**: The trait methods define clear contracts
//! - **Bricks (Implementations)**: Concrete repositories and mocks implement these traits
//! - **Regeneratable**: Can swap implementations without breaking dependents
//!
//! # Usage
//!
//! ```rust
//! use crate::infrastructure::persistence::repositories::traits::DocumentRepositoryTrait;
//!
//! async fn process_document<R: DocumentRepositoryTrait>(repo: &R, id: &str) {
//!     let doc = repo.find_by_id(id).await?;
//!     // ... process document
//! }
//! ```

use crate::domain::entities::Document;
use crate::shared::error::Result;
// Mention imports removed - migrated to DDD (MentionRepositoryTrait removed from this file)
// use crate::infrastructure::persistence::repositories::mention_repository::MentionRepository;
// use crate::infrastructure::persistence::repositories::mention_repository::{Mention, MentionWithContext};
use async_trait::async_trait;

/// Trait for document storage operations
///
/// Defines the contract for storing and retrieving documents from persistent storage.
/// Documents represent files that have been indexed in the system.
///
/// # Implementations
/// - `DocumentRepository`: Production SQLite implementation
/// - `MockDocumentRepository`: In-memory mock for testing
#[async_trait]
pub trait DocumentRepositoryTrait: Send + Sync {
    /// Create a new document record
    ///
    /// # Arguments
    /// * `file_path` - Absolute path to the file
    /// * `file_name` - Name of the file
    /// * `mime_type` - MIME type of the file
    /// * `size_bytes` - File size in bytes
    /// * `modified_at` - Last modification timestamp (RFC3339 format)
    /// * `checksum` - File content checksum (e.g., SHA-256)
    ///
    /// # Returns
    /// The created document with generated ID
    async fn create(
        &self,
        file_path: &str,
        file_name: &str,
        mime_type: &str,
        size_bytes: i64,
        modified_at: &str,
        checksum: &str,
    ) -> Result<Document>;

    /// Create or update a document based on file path
    ///
    /// If a document with the same file path exists, updates it.
    /// Otherwise, creates a new document.
    ///
    /// # Returns
    /// The document ID (newly created or existing)
    async fn upsert(
        &self,
        file_path: &str,
        file_name: &str,
        mime_type: &str,
        size_bytes: i64,
        modified_at: &str,
        checksum: &str,
    ) -> Result<String>;

    /// Find a document by its ID
    ///
    /// # Returns
    /// `Some(Document)` if found, `None` if not found
    async fn find_by_id(&self, id: &str) -> Result<Option<Document>>;

    /// Find a document by its file path
    ///
    /// # Returns
    /// `Some(Document)` if found, `None` if not found
    async fn find_by_path(&self, file_path: &str) -> Result<Option<Document>>;

    /// List all documents ordered by indexed timestamp (descending)
    async fn list_all(&self) -> Result<Vec<Document>>;

    /// Update the status of a document
    ///
    /// # Arguments
    /// * `id` - Document ID
    /// * `status` - New status (e.g., "indexed", "processing", "error")
    async fn update_status(&self, id: &str, status: &str) -> Result<()>;

    /// Delete a document by ID
    ///
    /// # Side Effects
    /// - Deletes associated chunks and embeddings (cascading delete)
    async fn delete(&self, id: &str) -> Result<()>;

    /// Count total number of documents
    async fn count(&self) -> Result<i64>;

    /// Check if a document exists for the given file path
    async fn exists(&self, file_path: &str) -> Result<bool>;

    /// Find documents matching a file path pattern (SQL LIKE)
    ///
    /// # Arguments
    /// * `pattern` - SQL LIKE pattern (e.g., "/docs/%", "%.pdf")
    async fn find_by_path_pattern(&self, pattern: &str) -> Result<Vec<Document>>;

    /// Alias for `find_by_id` (for compatibility)
    async fn get_by_id(&self, id: &str) -> Result<Option<Document>> {
        self.find_by_id(id).await
    }

    /// Alias for `list_all` (for compatibility)
    async fn get_all_documents(&self) -> Result<Vec<Document>> {
        self.list_all().await
    }
}

// Chunk Repository Trait - REMOVED (migrated to DDD)
// The old ChunkRepositoryTrait has been removed as part of the DDD migration.
// Chunks now use:
// - Domain entity: crate::domain::entities::chunk::Chunk
// - Repository: ChunkRepository implements RepositoryPort<Chunk> + ChunkRepositoryPort
// - Ports: crate::application::ports::{RepositoryPort, ChunkRepositoryPort}
// See chunk_repository.rs for the new implementation.

/*
/// Trait for embedding vector storage operations
///
/// Embeddings are vector representations of text chunks used for semantic search.
/// Each embedding is linked to a specific chunk.
///
/// # Implementations
/// - `EmbeddingRepository`: Production SQLite implementation (stores as BLOB)
/// - `MockEmbeddingRepository`: In-memory mock for testing
#[async_trait]
pub trait EmbeddingRepositoryTrait: Send + Sync {
    /// Create an embedding for a chunk
    ///
    /// # Arguments
    /// * `chunk_id` - ID of the chunk being embedded
    /// * `embedding` - Vector of floats representing the embedding
    /// * `model_name` - Name/version of the model used (e.g., DEFAULT_EMBEDDING_MODEL_NAME)
    ///
    /// # Returns
    /// The generated embedding ID
    async fn create(&self, chunk_id: &str, embedding: &[f32], model_name: &str) -> Result<String>;

    /// Create multiple embeddings in a single transaction
    ///
    /// More efficient for bulk operations.
    ///
    /// # Arguments
    /// * `chunk_embeddings` - Vector of (chunk_id, embedding) pairs
    /// * `model_name` - Name of the model used for all embeddings
    ///
    /// # Returns
    /// Vector of created embedding IDs in the same order
    async fn create_batch(
        &self,
        chunk_embeddings: Vec<(&str, Vec<f32>)>,
        model_name: &str,
    ) -> Result<Vec<String>>;

    /// Find embedding for a specific chunk
    async fn find_by_chunk(&self, chunk_id: &str) -> Result<Option<Embedding>>;

    /// Find all embeddings for a document (via its chunks)
    ///
    /// Results are ordered by chunk index.
    async fn find_by_document(&self, document_id: &str) -> Result<Vec<Embedding>>;

    /// Delete embedding for a specific chunk
    async fn delete_by_chunk(&self, chunk_id: &str) -> Result<()>;

    /// Delete all embeddings for a document
    async fn delete_by_document(&self, document_id: &str) -> Result<()>;

    /// Count total number of embeddings in the system
    async fn count(&self) -> Result<i64>;
}
*/

// Tag Repository Trait - REMOVED (migrated to DDD)
// The old TagRepositoryTrait has been removed as part of the DDD migration.
// Tags now use:
// - Domain entity: crate::domain::entities::tag::Tag
// - Repository: TagRepository implements RepositoryPort<TagEntity>
// - Service trait: crate::infrastructure::services::traits::TagRepositoryTrait
// See tag_repository.rs for the new implementation.

// Mention Repository Trait - MIGRATED TO DDD
// This trait has been DEPRECATED and replaced with MentionRepositoryPort.
// See src/application/ports/mention_repository_port.rs for the new interface.
// See src/infrastructure/services/traits/mention.rs for migration guide.
// Migration Path:
// - OLD: Arc<dyn MentionRepositoryTrait>
// - NEW: Arc<dyn MentionRepositoryPort>
// #[async_trait]
// pub trait MentionRepositoryTrait: Send + Sync {
//     /// Create a mention entity
//     ///
//     /// # Arguments
//     /// * `name` - Mention text/name
//     /// * `mention_type` - Type: "person", "concept", "wikilink"
//     /// * `metadata` - Optional JSON metadata
//     ///
//     /// Idempotent: if mention with same name exists, updates it
//     async fn create_mention(
//         &self,
//         name: &str,
//         mention_type: &str,
//         metadata: Option<&str>,
//     ) -> Result<Mention>;
//     /// Find mention by name
//     async fn find_mention_by_name(&self, name: &str) -> Result<Option<Mention>>;
//     /// Search mentions by partial name match
//     ///
//     /// # Arguments
//     /// * `query` - Search query (partial match)
//     /// * `limit` - Maximum number of results
//     async fn search_mentions(&self, query: &str, limit: i64) -> Result<Vec<Mention>>;
//     /// Get all mentions of a specific type
//     async fn get_mentions_by_type(&self, mention_type: &str) -> Result<Vec<Mention>>;
//     /// Link a mention to a document with context
//     ///
//     /// # Arguments
//     /// * `document_id` - Document containing the mention
//     /// * `mention_id` - Mention being referenced
//     /// * `context` - Surrounding text context (optional)
//     /// * `position` - Character position in document (optional)
//     async fn link_mention_to_document(
//         &self,
//         document_id: &str,
//         mention_id: &str,
//         context: Option<&str>,
//         position: Option<i64>,
//     ) -> Result<crate::repositories::mention_repository::DocumentMention>;
//     /// Get all mentions in a document with their context
//     ///
//     /// Results ordered by position in document
//     async fn get_mentions_for_document(&self, document_id: &str)
//         -> Result<Vec<MentionWithContext>>;
//     /// Get all documents that contain a specific mention
//     async fn get_documents_with_mention(&self, mention_id: &str) -> Result<Vec<String>>;
//     /// Remove all mention links for a document
//     async fn clear_document_mentions(&self, document_id: &str) -> Result<()>;
//     /// Delete a mention and all its links
//     async fn delete_mention(&self, id: &str) -> Result<()>;
//     /// Extract mentions from text using regex patterns
//     ///
//     /// Does not persist to database, only parses text.
//     ///
//     /// # Returns
//     /// HashMap mapping mention_type to Vec of (name, position) pairs
//     fn extract_mentions_from_text(&self, text: &str) -> HashMap<String, Vec<(String, usize)>>;
//     /// Extract mentions from text and store them with document links
//     ///
//     /// Combines extraction and persistence in one operation.
//     /// Clears existing mentions for the document first.
//     ///
//     /// # Returns
//     /// Vector of mentions with context that were extracted and stored
//     async fn extract_and_store_mentions(
//         &self,
//         document_id: &str,
//         text: &str,
//     ) -> Result<Vec<MentionWithContext>>;
// }

// Implement traits for production repositories

// Document Repository Trait Implementation - REMOVED
// Legacy DocumentRepositoryTrait implementation
// has been permanently removed as part of DDD migration.
// **Root Cause of Stack Overflow**: The old impl called DocumentRepository::create()
// as an associated function, but this method doesn't exist. The call resolved back to
// the trait method, creating infinite recursion → stack overflow.
// **Migration Complete**: All document repository operations now use the modern
// RepositoryPort<Document> pattern. See document_repository.rs for current implementation.
// **Test Infrastructure**: Test factories (tests/helpers/factories.rs) have been
// updated to use domain entities and RepositoryPort directly.
// This legacy trait is kept only for interface definition (other code may still
// reference the trait). The broken implementation has been completely removed.

// Old ChunkRepositoryTrait implementation removed - migrated to DDD
// See chunk_repository.rs for new RepositoryPort<Chunk> + ChunkRepositoryPort implementation

// Duplicate EmbeddingRepositoryTrait implementation removed
// Primary implementation is in embedding_repository.rs

// Old TagRepositoryTrait implementation removed - migrated to DDD
// See tag_repository.rs for new RepositoryPort<TagEntity> implementation

// Old MentionRepositoryTrait implementation removed - migrated to DDD
// See mention_repository.rs for new MentionRepositoryPort implementation
// This delegating impl is no longer needed
// #[async_trait]
// impl MentionRepositoryTrait for MentionRepository {
//     async fn create_mention(
//         &self,
//         name: &str,
//         mention_type: &str,
//         metadata: Option<&str>,
//     ) -> Result<Mention> {
//         MentionRepository::create_mention(self, name, mention_type, metadata).await
//     }
//     async fn find_mention_by_name(&self, name: &str) -> Result<Option<Mention>> {
//         MentionRepository::find_mention_by_name(self, name).await
//     }
//     async fn search_mentions(&self, query: &str, limit: i64) -> Result<Vec<Mention>> {
//         MentionRepository::search_mentions(self, query, limit).await
//     }
//     async fn get_mentions_by_type(&self, mention_type: &str) -> Result<Vec<Mention>> {
//         MentionRepository::get_mentions_by_type(self, mention_type).await
//     }
//     async fn link_mention_to_document(
//         &self,
//         document_id: &str,
//         mention_id: &str,
//         context: Option<&str>,
//         position: Option<i64>,
//     ) -> Result<crate::repositories::mention_repository::DocumentMention> {
//         MentionRepository::link_mention_to_document(
//             self,
//             document_id,
//             mention_id,
//             context,
//             position,
//         )
//         .await
//     }
//     async fn get_mentions_for_document(
//         &self,
//         document_id: &str,
//     ) -> Result<Vec<MentionWithContext>> {
//         MentionRepository::get_mentions_for_document(self, document_id).await
//     }
//     async fn get_documents_with_mention(&self, mention_id: &str) -> Result<Vec<String>> {
//         MentionRepository::get_documents_with_mention(self, mention_id).await
//     }
//     async fn clear_document_mentions(&self, document_id: &str) -> Result<()> {
//         MentionRepository::clear_document_mentions(self, document_id).await
//     }
//     async fn delete_mention(&self, id: &str) -> Result<()> {
//         MentionRepository::delete_mention(self, id).await
//     }
//     fn extract_mentions_from_text(&self, text: &str) -> HashMap<String, Vec<(String, usize)>> {
//         MentionRepository::extract_mentions_from_text(self, text)
//     }
//     async fn extract_and_store_mentions(
//         &self,
//         document_id: &str,
//         text: &str,
//     ) -> Result<Vec<MentionWithContext>> {
//         MentionRepository::extract_and_store_mentions(self, document_id, text).await
//     }
// }
