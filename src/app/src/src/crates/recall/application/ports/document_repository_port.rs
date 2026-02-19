//! Document repository port for document-specific queries.
//!
//! This port extends the generic RepositoryPort with document-specific operations.
//! While the generic repository handles basic CRUD, this port provides document
//! domain operations like path lookups and file existence checks.
//!
//! # Purpose
//!
//! - Provides document-specific repository operations
//! - Supports file path lookups by document ID
//! - Enables document-file path synchronization checks
//! - Maintains document-file relationships
//!
//! # Infrastructure Implementations
//!
//! - `SqliteDocumentRepository` - SQLite implementation
//! - `InMemoryDocumentRepository` - In-memory for testing
//!
//! # Example Usage
//!
//! ```rust
//! use crate::application::ports::DocumentRepositoryPort;
//!
//! async fn get_document_path(
//!     repo: &impl DocumentRepositoryPort,
//!     doc_id: &str,
//! ) -> Result<String> {
//!     repo.find_file_path_by_id(doc_id).await
//! }
//! ```

use crate::application::ports::repository_port::RepositoryPort;
use crate::domain::entities::Document;
use crate::shared::result::Result;
use async_trait::async_trait;

/// Port for document-specific repository operations.
///
/// Extends the generic RepositoryPort with document-specific operations.
/// Provides operations for working with document-file relationships
/// and document metadata lookups.
#[async_trait]
pub trait DocumentRepositoryPort: RepositoryPort<Document> {
    /// Find the file path for a document by its ID.
    ///
    /// # Arguments
    ///
    /// * `document_id` - The unique document identifier
    ///
    /// # Returns
    ///
    /// The file path associated with the document.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if document does not exist
    /// - `AppError::Database` if query fails
    /// - `AppError::InvalidInput` if document ID is invalid
    ///
    /// # Example
    ///
    /// ```rust
    /// let path = repo.find_file_path_by_id("doc-123").await?;
    /// println!("Document path: {}", path);
    /// ```
    async fn find_file_path_by_id(&self, document_id: &str) -> Result<String>;

    /// Check if a document exists by ID.
    ///
    /// # Arguments
    ///
    /// * `document_id` - The unique document identifier
    ///
    /// # Returns
    ///
    /// `true` if document exists, `false` otherwise.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    ///
    /// # Example
    ///
    /// ```rust
    /// if repo.document_exists("doc-123").await? {
    ///     println!("Document exists");
    /// }
    /// ```
    async fn document_exists(&self, document_id: &str) -> Result<bool>;

    /// Find document ID by file path.
    ///
    /// Reverse lookup from file path to document ID.
    ///
    /// # Arguments
    ///
    /// * `file_path` - The file path to search for
    ///
    /// # Returns
    ///
    /// `Some(document_id)` if found, `None` if not found.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    ///
    /// # Example
    ///
    /// ```rust
    /// if let Some(doc_id) = repo.find_id_by_path("/docs/file.txt").await? {
    ///     println!("Document ID: {}", doc_id);
    /// }
    /// ```
    async fn find_id_by_path(&self, file_path: &str) -> Result<Option<String>>;

    /// Delete a document by ID.
    ///
    /// # Arguments
    ///
    /// * `document_id` - The unique document identifier
    ///
    /// # Returns
    ///
    /// `Ok(())` on successful deletion.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if delete operation fails
    ///
    /// # Example
    ///
    /// ```rust
    /// repo.delete("doc-123").await?;
    /// println!("Document deleted");
    /// ```
    async fn delete(&self, document_id: &str) -> Result<()>;

    /// Find a document by its content checksum.
    ///
    /// This is used for duplicate detection during indexing to avoid re-indexing
    /// files that have already been processed.
    ///
    /// # Arguments
    ///
    /// * `checksum` - The content checksum to search for
    ///
    /// # Returns
    ///
    /// `Some(Document)` if a document with this checksum exists, `None` otherwise.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    ///
    /// # Example
    ///
    /// ```rust
    /// use crate::domain::value_objects::Checksum;
    ///
    /// let checksum = Checksum::new("abc123...".to_string())?;
    /// if let Some(doc) = repo.find_by_checksum(&checksum).await? {
    ///     println!("Document already indexed: {}", doc.id());
    /// }
    /// ```
    async fn find_by_checksum(
        &self,
        checksum: &crate::domain::value_objects::Checksum,
    ) -> Result<Option<Document>>;

    /// Count total number of indexed documents.
    ///
    /// # Returns
    ///
    /// Total count of documents in the repository.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    async fn count_documents(&self) -> Result<i64>;

    /// Count total number of chunks across all documents.
    ///
    /// # Returns
    ///
    /// Total count of chunks in the repository.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    async fn count_chunks(&self) -> Result<i64>;
}
