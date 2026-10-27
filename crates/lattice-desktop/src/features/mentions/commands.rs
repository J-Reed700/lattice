//! Mention Management Commands
//!
//! Thin command controllers for managing mentions (entity references within documents)
//! following DDD pattern. Mentions represent named entities like @mentions, #tags, or
//! `[[wikilinks]]` that create bidirectional links between documents.
//!
//! # Commands (7 total)
//!
//! - `extract_mentions` - Extract and store mentions from document content
//! - `search_mentions` - Search mentions by query text
//! - `get_mentions_for_document` - Get all mentions in a specific document
//! - `get_backlinks_for_mention` - Find documents mentioning a specific entity
//! - `get_mentions_by_type` - Filter mentions by type (person, tag, wikilink)
//! - `create_mention` - Manually create a new mention entity
//! - `delete_mention` - Delete a mention by ID
//!
//! # Mention Types
//!
//! - **@mentions**: Person references (e.g., @john_doe)
//! - **#tags**: Topic tags (e.g., #rust, #architecture)
//! - **wikilinks**: Document cross-references (e.g., `[[API Design]]`)
//! - **Custom**: Application-specific entity types
//!
//! # Architecture
//!
//! All commands are **thin wrappers** that delegate to mention use cases
//! with no intermediate business logic. This preserves the commands → use cases → repos flow.
//!
//! # Database Schema
//!
//! ```sql
//! CREATE TABLE mentions (
//!     id TEXT PRIMARY KEY,
//!     name TEXT NOT NULL UNIQUE,
//!     mention_type TEXT NOT NULL,
//!     metadata TEXT,
//!     created_at TEXT NOT NULL
//! );
//!
//! CREATE TABLE document_mentions (
//!     id TEXT PRIMARY KEY,
//!     document_id TEXT NOT NULL REFERENCES documents(id),
//!     mention_id TEXT NOT NULL REFERENCES mentions(id),
//!     context TEXT,
//!     position_start INTEGER,
//!     position_end INTEGER
//! );
//! ```

use crate::features::mentions::dto::{
    BacklinksResultDto, ExtractMentionsResultDto, GetMentionsForDocumentResultDto, MentionDto,
    SearchMentionsResultDto,
};
use crate::interfaces::di::Container;
use lattice_core::shared::error::Result;
use tauri::State;

/// Extract and store mentions from document content
///
/// Parses document content to identify mentions (@mentions, #tags, wikilinks) and stores
/// them in the database with contextual information. Returns the extracted mentions with their
/// positions and surrounding context for display or indexing.
///
/// # Mention Extraction
///
/// Automatically detects and extracts:
/// - **@mentions**: @username references
/// - **#tags**: #topic hashtags
/// - **wikilinks**: `[[Page Name]]` cross-references
///
/// Each mention includes:
/// - Position in document (start/end character offsets)
/// - Surrounding context (for preview/search)
/// - Mention type classification
///
/// # Architecture
///
/// Delegates to ExtractMentionsUseCase
pub async fn extract_mentions(
    document_id: String,
    content: String,
    container: State<'_, Container>,
) -> Result<ExtractMentionsResultDto> {
    let use_case = container.extract_mentions_use_case();
    use_case.execute(document_id, content).await
}

/// Search mentions by query text with optional result limit
///
/// Full-text search across mention names to find matching entities. Useful for autocomplete,
/// entity search, or finding mentions by partial name. Results ordered by relevance/name.
///
/// # Architecture
///
/// Delegates to SearchMentionsUseCase
pub async fn search_mentions(
    query: String,
    limit: Option<i64>,
    container: State<'_, Container>,
) -> Result<SearchMentionsResultDto> {
    let use_case = container.search_mentions_use_case();
    use_case.execute(query, limit).await
}

/// Get all mentions within a specific document
///
/// Retrieves all mentions found in a specific document with their positions and context.
/// Useful for displaying document relationships, entity references, or building backlink panels.
///
/// # Architecture
///
/// Delegates to GetMentionsForDocumentUseCase
pub async fn get_mentions_for_document(
    document_id: String,
    container: State<'_, Container>,
) -> Result<GetMentionsForDocumentResultDto> {
    let use_case = container.get_mentions_for_document_use_case();
    use_case.execute(document_id).await
}

/// Find all documents that mention a specific entity (backlinks)
///
/// Returns list of document IDs that contain references to the specified entity. Enables
/// bidirectional navigation from entity → documents. If mention doesn't exist, returns empty list.
///
/// # Behavior
///
/// 1. Find mention by name
/// 2. If found, query documents with that mention
/// 3. If not found, return empty list (no error)
///
/// # Architecture
///
/// Delegates to GetBacklinksUseCase
pub async fn get_backlinks_for_mention(
    mention_name: String,
    container: State<'_, Container>,
) -> Result<BacklinksResultDto> {
    let use_case = container.get_backlinks_use_case();
    use_case.execute(mention_name).await
}

/// Get all mentions of a specific type
///
/// Filter mentions by type (@mentions, #tags, wikilinks) to browse entities by category.
/// Useful for displaying entity lists, type-specific search, or category navigation.
///
/// # Architecture
///
/// Delegates to GetMentionsByTypeUseCase
pub async fn get_mentions_by_type(
    mention_type: String,
    container: State<'_, Container>,
) -> Result<Vec<MentionDto>> {
    let use_case = container.get_mentions_by_type_use_case();
    use_case.execute(mention_type).await
}

/// Create a new mention entity manually
///
/// Manually create a mention entity without requiring it to be extracted from document content.
/// Useful for pre-defining entities, creating custom entity types, or manual entity management.
///
/// # Architecture
///
/// Delegates to CreateMentionUseCase
pub async fn create_mention(
    name: String,
    mention_type: String,
    metadata: Option<String>,
    container: State<'_, Container>,
) -> Result<MentionDto> {
    let use_case = container.create_mention_use_case();
    use_case.execute(name, mention_type, metadata).await
}

/// Delete a mention entity by ID
///
/// Permanently deletes a mention entity and all its document associations. Use with caution
/// as this breaks backlinks. Consider keeping orphaned mentions for historical data preservation.
///
/// # Warning
///
/// Deleting a mention breaks all backlinks referencing this entity. Document associations
/// are removed via CASCADE DELETE.
///
/// # Architecture
///
/// Delegates to DeleteMentionUseCase
pub async fn delete_mention(mention_id: String, container: State<'_, Container>) -> Result<()> {
    let use_case = container.delete_mention_use_case();
    use_case.execute(mention_id).await
}
