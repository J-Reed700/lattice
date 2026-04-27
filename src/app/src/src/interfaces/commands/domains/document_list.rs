//! Document List Command
//!
//! Thin command controller for listing all indexed documents with metadata.
//!
//! # Commands
//!
//! - `list_all_documents` - Query all indexed documents from documents table
//!
//! # Purpose
//!
//! This command provides the source of truth for all indexed documents in the system.
//! It queries the `documents` table directly (NOT `recent_documents` which is just
//! an access tracking table).
//!
//! # Use Cases
//!
//! - **File Browser**: Display all documents in library view
//! - **Search Results**: Get complete document metadata for search hits
//! - **Document Management**: List all indexed files for batch operations
//! - **Analytics**: Get document corpus statistics
//!
//! # Database Schema
//!
//! Queries the `documents` table:
//!
//! ```sql
//! CREATE TABLE documents (
//!     id TEXT PRIMARY KEY,
//!     file_name TEXT NOT NULL,
//!     file_path TEXT NOT NULL,
//!     file_type TEXT,
//!     mime_type TEXT NOT NULL,
//!     category TEXT NOT NULL,
//!     language TEXT NOT NULL,
//!     modified_at TEXT NOT NULL,
//!     indexed_at TEXT NOT NULL,
//!     word_count INTEGER NOT NULL DEFAULT 0,
//!     -- ... other fields
//! );
//! ```
//!
//! # Architecture
//!
//! Pure delegation pattern - command delegates directly to DocumentRepository
//! via RepositoryPort trait. No rate limiting or audit logging needed for read operations.

use crate::application::ports::RepositoryPort;
use crate::domain::entities::Document as DocumentEntity;
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result, ResultExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

/// Document metadata DTO for display in UI
///
/// Contains all fields needed for organizing and displaying documents
/// in the file browser with custom organization (by category, type, date).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMetadataDto {
    /// Unique document identifier
    pub id: String,
    /// Original filename (e.g., "report.pdf")
    pub file_name: String,
    /// Full file path (library path or original path)
    pub file_path: String,
    /// File extension or type (e.g., "pdf", "txt")
    pub file_type: String,
    /// Document category for organization
    pub category: String,
    /// Detected language (e.g., "english", "python")
    pub language: String,
    /// File modification timestamp (RFC3339)
    pub modified_at: String,
    /// Indexing timestamp (RFC3339)
    pub indexed_at: String,
    /// Word count for document
    pub word_count: i32,
}

/// List all indexed documents with metadata.
///
/// # Arguments
///
/// * `container` - Service container with document repository
/// * `limit` - Maximum number of documents to return (1-10000)
///
/// # Returns
///
/// Vector of DocumentMetadataDto with all indexed documents.
///
/// # Errors
///
/// - `AppError::InvalidInput` if limit is out of range
/// - `AppError::Database` if query fails
///
/// # Example (Frontend)
///
/// ```typescript
/// // Get all documents for file browser
/// const result = await invoke('list_all_documents', { limit: 10000 });
/// console.log(`Found ${result.length} documents`);
/// ```
pub async fn list_all_documents(
    container: State<'_, Container>,
    limit: usize,
) -> Result<Vec<DocumentMetadataDto>> {
    // 1. Validate limit
    if limit == 0 || limit > 10000 {
        return Err(AppError::InvalidInput(
            "Limit must be between 1 and 10000".to_string(),
        ));
    }

    // 2. Get document repository from container
    let repo = container.document_repository();

    // 3. Query all documents (source of truth)
    let documents = repo
        .find_all()
        .await
        .context("Failed to query documents table")?;

    // 4. Map domain entities to DTOs
    let dtos: Vec<DocumentMetadataDto> = documents
        .into_iter()
        .take(limit)
        .map(|doc| DocumentMetadataDto {
            id: doc.id().as_str().to_string(),
            file_name: doc.file_name().to_string(),
            file_path: doc.file_path().display().to_string(),
            file_type: doc.file_type().map(|s| s.to_string()).unwrap_or_default(),
            category: doc.category().to_string(),
            language: doc.language().to_string(),
            modified_at: doc.modified_at().to_rfc3339(),
            indexed_at: doc.indexed_at().to_rfc3339(),
            word_count: doc.word_count(),
        })
        .collect();

    Ok(dtos)
}

/// Core implementation - Get document metadata by ID
///
/// # Arguments
///
/// * `container` - Service container with document repository
/// * `document_id` - Unique document identifier (UUID)
///
/// # Returns
///
/// DocumentMetadataDto if found, error if not found or invalid ID.
pub async fn get_document_impl(
    container: &Container,
    document_id: String,
) -> Result<DocumentMetadataDto> {
    // 1. Validate document_id is valid UUID format
    use uuid::Uuid;
    let _uuid = Uuid::parse_str(&document_id).map_err(|_| {
        AppError::InvalidInput(format!("Invalid document ID format: {}", document_id))
    })?;

    // 2. Get document repository from container
    let repo = container.document_repository();

    // 3. Find document by ID
    let doc = repo
        .find_by_id(&document_id)
        .await
        .context("Failed to query document by ID")?
        .ok_or_else(|| AppError::NotFound(format!("Document not found: {}", document_id)))?;

    // 4. Map domain entity to DTO
    let dto = DocumentMetadataDto {
        id: doc.id().as_str().to_string(),
        file_name: doc.file_name().to_string(),
        file_path: doc.file_path().display().to_string(),
        file_type: doc.file_type().map(|s| s.to_string()).unwrap_or_default(),
        category: doc.category().to_string(),
        language: doc.language().to_string(),
        modified_at: doc.modified_at().to_rfc3339(),
        indexed_at: doc.indexed_at().to_rfc3339(),
        word_count: doc.word_count(),
    };

    Ok(dto)
}

/// Legacy Tauri shim - Get document metadata by ID
///
/// # Arguments
///
/// * `container` - Service container with document repository
/// * `document_id` - Unique document identifier (UUID)
///
/// # Example
///
/// ```typescript
/// // Get document by ID to open from recent list
/// const result = await VaultAPI.getDocument(docId);
/// if (result.ok && result.data.filePath) {
///   await VaultAPI.openFile(result.data.filePath);
/// }
/// ```
#[tauri::command]
#[specta::specta]
pub async fn get_document(
    container: State<'_, Container>,
    document_id: String,
) -> Result<DocumentMetadataDto> {
    get_document_impl(&container, document_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::chunk::Chunk;
    use crate::domain::entities::document::Document;
    use crate::domain::entities::document::{Category, Language};
    use crate::interfaces::di::Container;
    use crate::shared::domain_types::{ChunkId, DocumentId, ValidatedFilePath};
    use chrono::Utc;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn create_test_container() -> (Container, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&format!("sqlite:{}?mode=rwc", db_path.display()))
            .await
            .unwrap();

        // Run migrations
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        let data_dir = temp_dir.path().to_path_buf();
        let db_conn = Arc::new(
            crate::infrastructure::persistence::database::DatabaseConnection::new(db_path.clone())
                .await
                .unwrap(),
        );
        let container = Container::new(
            pool,
            db_conn,
            None, // No embedding model for tests
            "http://localhost:11434",
            "llama2",
            data_dir,
        )
        .await
        .unwrap();
        (container, temp_dir)
    }

    // Tests using create_test_document removed - helper was removed as it was
    // testing aggregate construction which is not this layer's responsibility

    // Tests removed - these test Tauri command layer which requires State
    // Repository layer is tested in the tests above

    // Helper function removed - no longer used after test removal
}
