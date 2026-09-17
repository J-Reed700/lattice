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
    /// List document metadata without requiring indexed chunks or loading document bodies.
    /// Pending and failed documents remain visible in the inventory.
    async fn list_metadata(&self) -> Result<Vec<Document>> {
        self.find_all().await
    }

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

    /// Rename a document, touching only its metadata.
    ///
    /// Deliberately narrow. Renaming through the full aggregate `save()` path
    /// rewrites the document's children: the aggregate reconstructed for a
    /// rename carries no chunks, and the save deletes every existing chunk
    /// before inserting that empty set, cascading the embeddings away. The
    /// document then can't be loaded ("must have at least one chunk") and has
    /// silently vanished from search. A metadata-only edit must not go
    /// anywhere near child rows.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if no document has this id
    /// - `AppError::Database` if the update fails
    ///
    /// The default implementation refuses. Persistent repositories must
    /// override it; in-memory test doubles inherit a clear failure rather
    /// than silently succeeding without writing anything.
    async fn rename(&self, _document_id: &str, _new_file_name: &str) -> Result<()> {
        Err(crate::shared::error::AppError::Other(
            "rename is not supported by this repository implementation".to_string(),
        ))
    }

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

    /// How many documents reference this content checksum.
    ///
    /// The library stores one blob per checksum, so this is the question the
    /// blob garbage collector asks before removing anything: a checksum with
    /// no documents behind it is an orphan.
    ///
    /// The default derives the answer from [`Self::find_by_checksum`], which
    /// is enough for the only use (is it referenced at all?). Persistent
    /// repositories override it with a `COUNT(*)`.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if `checksum` is not a valid checksum
    /// - `AppError::Database` if query fails
    async fn count_by_checksum(&self, checksum: &str) -> Result<u64> {
        let checksum = crate::domain::value_objects::Checksum::new(checksum.to_string())?;
        Ok(u64::from(self.find_by_checksum(&checksum).await?.is_some()))
    }

    /// Every distinct content checksum referenced by a document.
    ///
    /// The startup sweep loads this set and removes every library blob whose
    /// hash is not in it.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    async fn list_checksums(&self) -> Result<Vec<String>> {
        let mut checksums: Vec<String> = self
            .find_all()
            .await?
            .iter()
            .map(|document| document.checksum().as_str().to_string())
            .collect();
        checksums.sort();
        checksums.dedup();
        Ok(checksums)
    }

    /// The content checksum of one document.
    ///
    /// Deliberately narrow, like [`Self::find_file_path_by_id`]: deletion needs
    /// the checksum of a document that may never have been chunked, and
    /// loading the aggregate for one column would refuse such a document
    /// ("must have at least one chunk") and make it undeletable.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if no document has this id
    /// - `AppError::Database` if query fails
    async fn find_checksum_by_id(&self, document_id: &str) -> Result<String> {
        self.find_by_id(document_id)
            .await?
            .map(|document| document.checksum().as_str().to_string())
            .ok_or_else(|| {
                crate::shared::error::AppError::NotFound(format!(
                    "Document not found: {document_id}"
                ))
            })
    }

    /// Find documents with a SQL-level LIMIT applied.
    ///
    /// Preferred over `find_all` when the caller doesn't need the entire table;
    /// the LIMIT is applied at the SQL level so the database doesn't materialize
    /// rows that will be discarded. Documents are returned in the same order as
    /// `find_all` (most recently indexed first).
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of documents to return.
    ///
    /// # Returns
    ///
    /// Up to `limit` documents, ordered by `indexed_at DESC`.
    ///
    /// # Errors
    ///
    /// - `AppError::Database` if query fails
    async fn find_all_paginated(&self, limit: usize) -> Result<Vec<Document>>;
}
