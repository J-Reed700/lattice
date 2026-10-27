//! Recent Documents Tracking Commands
//!
//! Thin command controllers for tracking document access patterns and maintaining a list of
//! recently accessed documents with LRU (Least Recently Used) eviction strategy.
//!
//! # Commands (4 total)
//!
//! - `recent_document_operation` - Unified dispatcher (track/getRecent/clear)
//! - `track_document_access` - Track document access with timestamp and count
//! - `get_recent_documents` - Get recently accessed documents with metadata (served by the
//!   file plugin over `RecentDocumentsRepositoryPort`)
//! - `clear_recent_documents` - Clear all recent document history
//!
//! # Features
//!
//! - **Access Tracking**: Record document access with timestamps
//! - **Access Analytics**: Maintain cumulative access count per document
//! - **LRU Eviction**: Automatically keeps only last 50 documents (configurable)
//! - **Metadata Enrichment**: Recent documents include name, path, type
//! - **Idempotent Tracking**: Safe to track same document multiple times
//!
//! # LRU Eviction Strategy
//!
//! When tracking a new document that would exceed MAX_RECENT_DOCUMENTS (50):
//! 1. Sort recent documents by `last_accessed_at` DESC
//! 2. Keep top 50 documents
//! 3. Delete all others (oldest accessed)
//!
//! This ensures the list contains only the most recently accessed documents.
//!
//! # Database Schema
//!
//! ```sql
//! CREATE TABLE recent_documents (
//!     id TEXT PRIMARY KEY,
//!     document_id TEXT NOT NULL REFERENCES documents(id),
//!     last_accessed_at TEXT NOT NULL,
//!     access_count INTEGER NOT NULL DEFAULT 1
//! );
//! ```
//!
//! # Architecture
//!
//! Pure delegation pattern with no rate limiting or audit logging. Commands delegate directly
//! to database layer for simple CRUD operations on recent documents tracking table.

use crate::interfaces::di::Container;
use lattice_core::application::contracts::recent_documents::RecentDocument;
use lattice_core::application::ports::RecentDocumentsRepositoryPort;
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum RecentDocumentOperation {
    Track { document_id: String },
    GetRecent { limit: usize },
    Clear,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum RecentDocumentResponse {
    Tracked,
    Documents(Vec<RecentDocument>),
    Cleared,
}

/// Unified dispatcher for recent document operations (track/getRecent/clear)
///
/// Single command that handles all recent document operations via enum dispatch pattern.
/// Useful for TypeScript callers that want to batch operations or use type-safe dispatching.
///
/// # Operation Types
///
/// - **Track**: Record document access (updates timestamp, increments count, applies LRU)
/// - **GetRecent**: Retrieve recent documents list (limit capped at 100)
/// - **Clear**: Delete all recent document history
///
/// # Architecture
///
/// Enum dispatch pattern delegating to internal implementations
pub async fn recent_document_operation(
    operation: RecentDocumentOperation,
    container: State<'_, Container>,
) -> Result<RecentDocumentResponse, AppError> {
    let repository = container.recent_documents_repository();
    match operation {
        RecentDocumentOperation::Track { document_id } => {
            track_document_access_internal(&document_id, repository.as_ref()).await?;
            Ok(RecentDocumentResponse::Tracked)
        }
        RecentDocumentOperation::GetRecent { limit } => {
            let docs = get_recent_documents_internal(limit, repository.as_ref()).await?;
            Ok(RecentDocumentResponse::Documents(docs))
        }
        RecentDocumentOperation::Clear => {
            clear_recent_documents_internal(repository.as_ref()).await?;
            Ok(RecentDocumentResponse::Cleared)
        }
    }
}

/// Track document access with timestamp and count update
///
/// Records document access by updating timestamp and incrementing access count. If document
/// is already tracked, updates existing record. If not tracked, creates new record. Automatically
/// applies LRU eviction to maintain MAX_RECENT_DOCUMENTS (50) limit.
///
/// # Behavior
///
/// 1. **Validation**: Check document exists in documents table
/// 2. **Check Existing**: Query recent_documents for existing record
/// 3. **Update or Insert**:
///    - If exists: UPDATE last_accessed_at, increment access_count
///    - If not exists: INSERT new record with access_count = 1
/// 4. **LRU Eviction**: Delete oldest documents beyond MAX_RECENT_DOCUMENTS
///
/// # Idempotency
///
/// Safe to call multiple times for same document - each call increments access_count
/// and updates timestamp. No duplicate records created.
///
/// # Architecture
///
/// Thin controller delegating to `track_document_access_internal`
pub async fn track_document_access(
    document_id: String,
    container: State<'_, Container>,
) -> Result<(), AppError> {
    let repository = container.recent_documents_repository();
    track_document_access_internal(&document_id, repository.as_ref())
        .await
        .map_err(|e| AppError::Other(format!("Failed to track document access: {}", e)))
}

async fn track_document_access_internal(
    document_id: &str,
    repository: &dyn RecentDocumentsRepositoryPort,
) -> Result<()> {
    repository.track_access(document_id).await
}

async fn get_recent_documents_internal(
    limit: usize,
    repository: &dyn RecentDocumentsRepositoryPort,
) -> Result<Vec<RecentDocument>> {
    Ok(repository
        .get_recent_documents(limit)
        .await?
        .into_iter()
        .map(RecentDocument::from)
        .collect())
}

/// Clear all recent document history
///
/// Deletes all records from recent_documents table. Use for privacy, cleanup, or reset scenarios.
/// This operation is permanent and cannot be undone. Actual documents in documents table remain
/// unaffected - only the recent access tracking history is cleared.
///
/// # Query
///
/// ```sql
/// DELETE FROM recent_documents
/// ```
///
/// # Warning
///
/// This operation is **permanent** and **irreversible**. Recent document history cannot be
/// recovered after clearing. Use with caution in production environments.
///
/// # Architecture
///
/// Thin controller delegating to `clear_recent_documents_internal`
pub async fn clear_recent_documents(container: State<'_, Container>) -> Result<(), AppError> {
    let repository = container.recent_documents_repository();
    clear_recent_documents_internal(repository.as_ref())
        .await
        .map_err(|e| AppError::Other(format!("Failed to clear recent documents: {}", e)))
}

async fn clear_recent_documents_internal(
    repository: &dyn RecentDocumentsRepositoryPort,
) -> Result<()> {
    repository.clear_recent_history(None).await?;
    Ok(())
}

// `list_all_documents` lives in document_list.rs, which returns the full
// DocumentMetadataDto.

#[cfg(test)]
mod tests {

    // Tests would go here - integration tests with test database
}
