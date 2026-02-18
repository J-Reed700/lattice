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
//!
//! # Use Cases
//!
//! - **Document Linking**: Create bidirectional links between documents
//! - **Entity Tracking**: Track mentions of people, projects, topics across corpus
//! - **Backlink Navigation**: Navigate from entity to all mentioning documents
//! - **Search Enhancement**: Find documents by mentioned entities
//! - **Knowledge Graph**: Build entity relationship graph from mentions

use crate::application::dtos::mention_dto::{
    BacklinksResultDto, ExtractMentionsResultDto, GetMentionsForDocumentResultDto, MentionDto,
    SearchMentionsResultDto,
};
use crate::interfaces::di::Container;
use crate::shared::error::Result;
use tauri::State;

/// Extract and store mentions from document content
///
/// Parses document content to identify mentions (@mentions, #tags, wikilinks) and stores
/// them in the database with contextual information. Returns the extracted mentions with their
/// positions and surrounding context for display or indexing.
///
/// # Arguments
///
/// * `document_id` - ID of document being processed
/// * `content` - Full document content to scan for mentions
/// * `container` - Service container with mention repository
///
/// # Returns
///
/// * `Ok(ExtractMentionsResponse)` - Extracted mentions with count
/// * `Err(AppError)` - Extraction or storage failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface MentionWithContext {
///   id: string;
///   name: string;
///   mentionType: string;
///   context: string;
///   positionStart: number;
///   positionEnd: number;
/// }
///
/// interface ExtractMentionsResponse {
///   mentions: MentionWithContext[];
///   count: number;
/// }
///
/// // Extract mentions from document content
/// const result = await invoke<ExtractMentionsResponse>('extract_mentions', {
///   documentId: 'doc-123',
///   content: 'Meeting with @alice about #rust and \[\[API Design\]\]'
/// });
///
/// console.log(`Found ${result.count} mentions:`, result.mentions);
/// // Output: Found 3 mentions: [@alice, #rust, \[\[API Design\]\]]
///
/// // Process extracted mentions
/// result.mentions.forEach(mention => {
///   console.log(`${mention.name} (${mention.mentionType}) at ${mention.positionStart}`);
/// });
/// ```
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
/// # Use Cases
///
/// - **Document Indexing**: Extract mentions during document processing
/// - **Link Creation**: Build bidirectional document links
/// - **Entity Tracking**: Track entity mentions across corpus
/// - **Autocomplete**: Suggest existing entities while typing
///
/// # Performance
///
/// - **Extraction Time**: ~10-50ms (depends on content length)
/// - **Regex Parsing**: Efficient pattern matching
/// - **Batch Insert**: All mentions inserted in single transaction
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
/// # Arguments
///
/// * `query` - Search query text (partial match supported)
/// * `limit` - Max results to return (default: 20)
/// * `container` - Service container with mention repository
///
/// # Returns
///
/// * `Ok(SearchMentionsResponse)` - Matching mentions
/// * `Err(AppError)` - Search failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface MentionData {
///   id: string;
///   name: string;
///   mentionType: string;
///   metadata?: string;
/// }
///
/// interface SearchMentionsResponse {
///   mentions: MentionData[];
/// }
///
/// // Search mentions by query
/// const result = await invoke<SearchMentionsResponse>('search_mentions', {
///   query: 'rust',
///   limit: 10
/// });
///
/// console.log(`Found ${result.mentions.length} mentions matching "rust"`);
///
/// // Autocomplete implementation
/// const autocompleteEntities = async (input: string) => {
///   const result = await invoke<SearchMentionsResponse>('search_mentions', {
///     query: input,
///     limit: 5
///   });
///   return result.mentions.map(m => m.name);
/// };
/// ```
///
/// # Use Cases
///
/// - **Autocomplete**: Suggest entities while typing
/// - **Entity Search**: Find entities by partial name
/// - **Browse Entities**: Explore available mentions
///
/// # Performance
///
/// - **Search Time**: ~5-20ms (depends on mention count)
/// - **FTS Index**: Uses full-text search index for fast matching
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
/// # Arguments
///
/// * `document_id` - ID of document to query
/// * `container` - Service container with mention repository
///
/// # Returns
///
/// * `Ok(Vec<MentionWithContextData>)` - Mentions with context (empty if none)
/// * `Err(AppError)` - Query failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Get mentions in document
/// const mentions = await invoke<MentionWithContext[]>('get_mentions_for_document', {
///   documentId: 'doc-123'
/// });
///
/// console.log(`Document references ${mentions.length} entities`);
///
/// // Display backlinks panel
/// const displayBacklinks = async (docId: string) => {
///   const mentions = await invoke<MentionWithContext[]>('get_mentions_for_document', {
///     documentId: docId
///   });
///   return mentions.map(m => `${m.name} (${m.mentionType})`);
/// };
/// ```
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
/// # Arguments
///
/// * `mention_name` - Name of entity to find backlinks for
/// * `container` - Service container with mention repository
///
/// # Returns
///
/// * `Ok(BacklinksResponse)` - Document IDs mentioning entity (empty if none)
/// * `Err(AppError)` - Query failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface BacklinksResponse {
///   documentIds: string[];
/// }
///
/// // Get backlinks for entity
/// const result = await invoke<BacklinksResponse>('get_backlinks_for_mention', {
///   mentionName: '@alice'
/// });
///
/// console.log(`@alice is mentioned in ${result.documentIds.length} documents`);
///
/// // Build backlink graph
/// const buildBacklinkGraph = async (entityName: string) => {
///   const backlinks = await invoke<BacklinksResponse>('get_backlinks_for_mention', {
///     mentionName: entityName
///   });
///
///   // Load each document to build graph
///   const docs = await Promise.all(
///     backlinks.documentIds.map(id => loadDocument(id))
///   );
///
///   return { entity: entityName, documents: docs };
/// };
/// ```
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
/// # Arguments
///
/// * `mention_type` - Type filter (e.g., "person", "tag", "wikilink")
/// * `container` - Service container with mention repository
///
/// # Returns
///
/// * `Ok(Vec<MentionData>)` - Mentions of specified type (empty if none)
/// * `Err(AppError)` - Query failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Get all person mentions
/// const people = await invoke<MentionData[]>('get_mentions_by_type', {
///   mentionType: 'person'
/// });
///
/// console.log(`Found ${people.length} people mentioned in corpus`);
///
/// // Get all tags
/// const tags = await invoke<MentionData[]>('get_mentions_by_type', {
///   mentionType: 'tag'
/// });
///
/// // Display tag cloud
/// const displayTagCloud = async () => {
///   const tags = await invoke<MentionData[]>('get_mentions_by_type', {
///     mentionType: 'tag'
///   });
///   return tags.map(tag => tag.name);
/// };
/// ```
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
/// # Arguments
///
/// * `name` - Mention name (e.g., "@alice", "#rust", "[[API Design]]")
/// * `mention_type` - Entity type classification
/// * `metadata` - Optional JSON metadata
/// * `container` - Service container with mention repository
///
/// # Returns
///
/// * `Ok(MentionData)` - Created mention with generated ID
/// * `Err(AppError)` - Creation failed (duplicate name, validation error)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Create person mention
/// const person = await invoke<MentionData>('create_mention', {
///   name: '@alice',
///   mentionType: 'person',
///   metadata: JSON.stringify({ role: 'engineer', team: 'backend' })
/// });
///
/// console.log('Created mention:', person.id);
///
/// // Pre-define entities for autocomplete
/// const predefineTags = async (tags: string[]) => {
///   for (const tag of tags) {
///     await invoke('create_mention', {
///       name: `#${tag}`,
///       mentionType: 'tag',
///       metadata: null
///     });
///   }
/// };
/// ```
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
/// # Arguments
///
/// * `mention_id` - ID of mention to delete
/// * `container` - Service container with mention repository
///
/// # Returns
///
/// * `Ok(())` - Mention deleted successfully
/// * `Err(AppError)` - Deletion failed (not found, foreign key constraint)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Delete mention
/// await invoke('delete_mention', { mentionId: 'mention-123' });
/// console.log('Mention deleted');
///
/// // Cleanup unused mentions
/// const cleanupOrphanedMentions = async () => {
///   const allMentions = await invoke<MentionData[]>('search_mentions', {
///     query: '',
///     limit: 1000
///   });
///
///   for (const mention of allMentions) {
///     const backlinks = await invoke<BacklinksResponse>('get_backlinks_for_mention', {
///       mentionName: mention.name
///     });
///
///     if (backlinks.documentIds.length === 0) {
///       await invoke('delete_mention', { mentionId: mention.id });
///     }
///   }
/// };
/// ```
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
