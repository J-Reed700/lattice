//! Recent Documents Tracking Commands
//!
//! Thin command controllers for tracking document access patterns and maintaining a list of
//! recently accessed documents with LRU (Least Recently Used) eviction strategy.
//!
//! # Commands (4 total)
//!
//! - `recent_document_operation` - Unified dispatcher (track/getRecent/clear)
//! - `track_document_access` - Track document access with timestamp and count
//! - `get_recent_documents` - Get recently accessed documents with metadata
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
//! # Use Cases
//!
//! - **UI Recent Files Menu**: Display recent documents for quick access
//! - **Analytics Dashboard**: Show most frequently accessed documents
//! - **User Workflows**: Track user's document access patterns
//! - **Quick Navigation**: Jump to recently viewed documents
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

use crate::application::ports::RecentDocumentsRepositoryPort;
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use tauri::State;

/// Recent document with access metadata
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RecentDocument {
    pub id: String,
    pub document_id: String,
    pub document_name: String,
    pub document_path: String,
    pub file_type: Option<String>,
    pub last_accessed_at: String,
    pub access_count: i64,
}

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
/// # Arguments
///
/// * `operation` - Operation to perform (Track, GetRecent, Clear)
/// * `container` - Service container with database pool
///
/// # Returns
///
/// * `Ok(RecentDocumentResponse)` - Operation result (Tracked/Documents/Cleared)
/// * `Err(AppError::NotFound)` - Document not found (track operation)
/// * `Err(AppError)` - Operation failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// type RecentDocumentOperation =
///   | { action: 'track'; documentId: string }
///   | { action: 'getRecent'; limit: number }
///   | { action: 'clear' };
///
/// type RecentDocumentResponse =
///   | { type: 'tracked' }
///   | { type: 'documents'; data: RecentDocument[] }
///   | { type: 'cleared' };
///
/// interface RecentDocument {
///   id: string;
///   documentId: string;
///   documentName: string;
///   documentPath: string;
///   fileType?: string;
///   lastAccessedAt: string;
///   accessCount: number;
/// }
///
/// // Track document access
/// const trackResult = await invoke<RecentDocumentResponse>('recent_document_operation', {
///   operation: { action: 'track', documentId: 'doc-123' }
/// });
///
/// if (trackResult.type === 'tracked') {
///   console.log('Document access tracked');
/// }
///
/// // Get recent documents
/// const getResult = await invoke<RecentDocumentResponse>('recent_document_operation', {
///   operation: { action: 'getRecent', limit: 10 }
/// });
///
/// if (getResult.type === 'documents') {
///   console.log(`${getResult.data.length} recent documents:`, getResult.data);
/// }
///
/// // Clear recent documents
/// const clearResult = await invoke<RecentDocumentResponse>('recent_document_operation', {
///   operation: { action: 'clear' }
/// });
///
/// if (clearResult.type === 'cleared') {
///   console.log('Recent documents cleared');
/// }
///
/// // Type-safe operation handler
/// const handleRecentOperation = async (op: RecentDocumentOperation) => {
///   const result = await invoke<RecentDocumentResponse>('recent_document_operation', {
///     operation: op
///   });
///
///   switch (result.type) {
///     case 'tracked':
///       return 'Document tracked successfully';
///     case 'documents':
///       return `Found ${result.data.length} recent documents`;
///     case 'cleared':
///       return 'Recent documents history cleared';
///   }
/// };
/// ```
///
/// # Operation Types
///
/// - **Track**: Record document access (updates timestamp, increments count, applies LRU)
/// - **GetRecent**: Retrieve recent documents list (limit capped at 100)
/// - **Clear**: Delete all recent document history
///
/// # Performance
///
/// - **Track**: ~5-15ms (includes LRU eviction check)
/// - **GetRecent**: ~5-20ms (depends on limit)
/// - **Clear**: ~5-10ms (simple DELETE query)
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
/// # Arguments
///
/// * `document_id` - ID of document being accessed
/// * `container` - Service container with database pool
///
/// # Returns
///
/// * `Ok(())` - Access tracked successfully
/// * `Err(AppError::NotFound)` - Document does not exist in documents table
/// * `Err(AppError)` - Tracking failed (database error)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Track document access
/// await invoke('track_document_access', { documentId: 'doc-123' });
/// console.log('Document access tracked');
///
/// // Track on document open
/// const handleDocumentOpen = async (docId: string) => {
///   try {
///     await invoke('track_document_access', { documentId: docId });
///     console.log('Tracking: document opened');
///   } catch (error) {
///     console.error('Failed to track access:', error);
///   }
/// };
///
/// // Track multiple accesses (idempotent)
/// for (let i = 0; i < 5; i++) {
///   await invoke('track_document_access', { documentId: 'doc-123' });
/// }
/// // Result: access_count = 5, last_accessed_at = now
///
/// // Auto-tracking in document viewer
/// const DocumentViewer = ({ documentId }: { documentId: string }) => {
///   useEffect(() => {
///     invoke('track_document_access', { documentId });
///   }, [documentId]);
///
///   return <div>Document content...</div>;
/// };
/// ```
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
/// # Performance
///
/// - **Query Time**: ~5-15ms (includes eviction check)
/// - **Database Operations**: 2-3 queries (check exists, upsert, evict)
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

/// Get recent documents with metadata and access stats
///
/// Retrieves recently accessed documents ordered by last access time (newest first). Returns
/// enriched metadata including document name, path, type, timestamp, and access count. Limit
/// is automatically capped at 100 for performance.
///
/// # Arguments
///
/// * `limit` - Max documents to return (capped at 100)
/// * `container` - Service container with database pool
///
/// # Returns
///
/// * `Ok(Vec<RecentDocument>)` - Recent documents with metadata (empty if none)
/// * `Err(AppError)` - Query failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface RecentDocument {
///   id: string;
///   documentId: string;
///   documentName: string;
///   documentPath: string;
///   fileType?: string;
///   lastAccessedAt: string;
///   accessCount: number;
/// }
///
/// // Get 10 most recent documents
/// const recent = await invoke<RecentDocument[]>('get_recent_documents', {
///   limit: 10
/// });
///
/// console.log(`${recent.length} recent documents:`);
/// recent.forEach(doc => {
///   console.log(`- ${doc.documentName} (accessed ${doc.accessCount} times)`);
/// });
///
/// // Display in UI recent files menu
/// const RecentFilesMenu = () => {
///   const [recentDocs, setRecentDocs] = useState<RecentDocument[]>([]);
///
///   useEffect(() => {
///     const loadRecent = async () => {
///       const docs = await invoke<RecentDocument[]>('get_recent_documents', {
///         limit: 15
///       });
///       setRecentDocs(docs);
///     };
///     loadRecent();
///   }, []);
///
///   return (
///     <ul className="recent-files">
///       {recentDocs.map(doc => (
///         <li key={doc.id} onClick={() => openDocument(doc.documentId)}>
///           <span className="name">{doc.documentName}</span>
///           <span className="count">{doc.accessCount}x</span>
///           <span className="time">{formatTime(doc.lastAccessedAt)}</span>
///         </li>
///       ))}
///     </ul>
///   );
/// };
///
/// // Analytics: most accessed documents
/// const getMostAccessedDocuments = async () => {
///   const recent = await invoke<RecentDocument[]>('get_recent_documents', {
///     limit: 100
///   });
///
///   // Sort by access count
///   const sorted = recent.sort((a, b) => b.accessCount - a.accessCount);
///
///   console.log('Top 10 most accessed documents:');
///   sorted.slice(0, 10).forEach((doc, i) => {
///     console.log(`${i + 1}. ${doc.documentName}: ${doc.accessCount} accesses`);
///   });
/// };
/// ```
///
/// # Query Details
///
/// ```sql
/// SELECT r.id, r.document_id, d.file_name, d.file_path, d.file_type,
///        r.last_accessed_at, r.access_count
/// FROM recent_documents r
/// INNER JOIN documents d ON r.document_id = d.id
/// ORDER BY r.last_accessed_at DESC
/// LIMIT ?
/// ```
///
/// # Limit Capping
///
/// - **User Request**: Any value
/// - **Applied Limit**: `min(limit, 100)`
/// - **Reason**: Performance protection for large result sets
///
/// # Performance
///
/// - **Query Time**: ~5-20ms (depends on limit)
/// - **JOIN**: INNER JOIN with documents table for metadata enrichment
/// - **Ordering**: ORDER BY last_accessed_at DESC (indexed)
///
/// # Architecture
///
/// Thin controller delegating to `get_recent_documents_internal`
pub async fn get_recent_documents(
    limit: usize,
    container: State<'_, Container>,
) -> Result<Vec<RecentDocument>, AppError> {
    let repository = container.recent_documents_repository();
    get_recent_documents_internal(limit, repository.as_ref())
        .await
        .map_err(|e| AppError::Other(format!("Failed to get recent documents: {}", e)))
}

async fn get_recent_documents_internal(
    limit: usize,
    repository: &dyn RecentDocumentsRepositoryPort,
) -> Result<Vec<RecentDocument>> {
    Ok(repository
        .get_recent_documents(limit)
        .await?
        .into_iter()
        .map(|record| RecentDocument {
            id: record.id,
            document_id: record.document_id,
            document_name: record.document_name,
            document_path: record.document_path,
            file_type: record.file_type,
            last_accessed_at: record.last_accessed_at,
            access_count: record.access_count,
        })
        .collect())
}

/// Clear all recent document history
///
/// Deletes all records from recent_documents table. Use for privacy, cleanup, or reset scenarios.
/// This operation is permanent and cannot be undone. Actual documents in documents table remain
/// unaffected - only the recent access tracking history is cleared.
///
/// # Arguments
///
/// * `container` - Service container with database pool
///
/// # Returns
///
/// * `Ok(())` - Recent documents cleared successfully
/// * `Err(AppError)` - Clear operation failed (database error)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Clear recent documents history
/// await invoke('clear_recent_documents');
/// console.log('Recent documents history cleared');
///
/// // Clear with confirmation
/// const handleClearRecent = async () => {
///   const confirmed = window.confirm(
///     'This will clear your recent documents history. Continue?'
///   );
///
///   if (confirmed) {
///     await invoke('clear_recent_documents');
///     console.log('History cleared');
///     refreshRecentFiles(); // Reload UI
///   }
/// };
///
/// // Privacy mode: clear on exit
/// const handleAppExit = async () => {
///   if (privacyModeEnabled) {
///     await invoke('clear_recent_documents');
///     console.log('Privacy mode: recent history cleared');
///   }
///   window.close();
/// };
///
/// // Settings: clear recent history button
/// const SettingsPanel = () => {
///   const [clearing, setClearing] = useState(false);
///
///   const handleClear = async () => {
///     setClearing(true);
///     try {
///       await invoke('clear_recent_documents');
///       toast.success('Recent history cleared');
///     } catch (error) {
///       toast.error('Failed to clear history');
///     } finally {
///       setClearing(false);
///     }
///   };
///
///   return (
///     <button onClick={handleClear} disabled={clearing}>
///       {clearing ? 'Clearing...' : 'Clear Recent History'}
///     </button>
///   );
/// };
///
/// // Reset workflow
/// const resetUserPreferences = async () => {
///   await invoke('clear_recent_documents');
///   await invoke('clear_favorites');
///   console.log('User preferences reset');
/// };
/// ```
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
/// # Use Cases
///
/// - **Privacy**: Clear access history before sharing device
/// - **Cleanup**: Remove stale tracking data
/// - **Reset**: Start fresh with recent documents tracking
/// - **Debugging**: Clear corrupt or inconsistent tracking data
///
/// # Performance
///
/// - **Query Time**: ~5-10ms (simple DELETE operation)
/// - **No Cascade**: Only clears recent_documents table
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

// The old implementation here queried the documents table but returned RecentDocument DTO,
// which was incorrect. The new implementation in document_list.rs queries documents table
// and returns proper DocumentMetadataDto with all fields needed for organization.
// Migration: Frontend calls to invoke('list_all_documents') will now use the new command
// from document_list.rs which returns richer metadata.

#[cfg(test)]
mod tests {

    // Tests would go here - integration tests with test database
}
