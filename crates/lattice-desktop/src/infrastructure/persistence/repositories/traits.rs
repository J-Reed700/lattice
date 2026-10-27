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

use lattice_core::domain::entities::Document;
use lattice_core::shared::error::Result;
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
    async fn find_by_id(&self, id: &str) -> Result<Option<Document>>;

    /// Find a document by its file path
    async fn find_by_path(&self, file_path: &str) -> Result<Option<Document>>;

    /// List all documents ordered by indexed timestamp (descending)
    async fn list_all(&self) -> Result<Vec<Document>>;

    /// Update the status of a document
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
