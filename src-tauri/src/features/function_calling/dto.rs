//! # Function Calling DTOs
//!
//! Data Transfer Objects for LLM function calling tools.
//!
//! These DTOs define the contracts for function calling interfaces that allow
//! LLMs to interact with the Lattice system through structured tool use.
//!
//! ## Design Philosophy (Bricks and Studs)
//!
//! - **Minimal**: Only essential fields
//! - **Clear**: Descriptive names and comprehensive documentation
//! - **Stable**: Versioned schemas for breaking changes
//! - **Validated**: Strong typing with serde

use chrono::{DateTime, Utc};
use lattice_core::application::contracts::web::default_max_results;
use serde::{Deserialize, Serialize};

/// Input for semantic_search function.
///
/// Searches the user's document lattice using semantic similarity.
/// Use this when the user asks to find, search, or retrieve information
/// from their documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchInput {
    /// Search query describing what to find
    pub query: String,

    /// Maximum number of results to return (1-50)
    #[serde(default = "default_limit")]
    pub limit: usize,

    /// Results to skip: 0, or the `next_offset` a previous search named, so
    /// results past the first page are reachable.
    #[serde(default)]
    pub offset: usize,

    /// Minimum similarity score (0.0-1.0). Lower = more results
    #[serde(default = "default_threshold")]
    pub threshold: f32,

    /// Search algorithm: semantic, keyword, or hybrid
    #[serde(default)]
    pub search_mode: SearchMode,

    /// Filter by file extensions (e.g., ["pdf", "txt"])
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_types: Option<Vec<String>>,

    /// Only return documents modified after this date
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_from: Option<DateTime<Utc>>,

    /// Only return documents modified before this date
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_to: Option<DateTime<Utc>>,
}

fn default_limit() -> usize {
    10
}

fn default_threshold() -> f32 {
    0.3
}

/// Search mode for algorithm selection.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum SearchMode {
    /// Semantic search using embeddings
    Semantic,
    /// Keyword search using BM25
    Keyword,
    /// Hybrid search combining both
    #[default]
    Hybrid,
}

/// A single document search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentResult {
    /// Unique document identifier
    pub document_id: String,

    /// Document filename
    pub filename: String,

    /// Full path to document
    pub file_path: String,

    /// MIME type (e.g., 'application/pdf')
    pub mime_type: String,

    /// Relevance score (0.0-1.0)
    pub score: f32,

    /// Relevant excerpt from the document
    pub snippet: String,

    /// Chunk position if result is from a chunk (0-based)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_index: Option<usize>,

    /// Last modification timestamp
    pub modified_at: DateTime<Utc>,

    /// File size in bytes
    pub size_bytes: i64,
}

/// A single matched excerpt within a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentEvidenceMatch {
    /// Chunk position if available (0-based)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_index: Option<usize>,

    /// Match score for this excerpt
    pub score: f32,

    /// Excerpt text for this match
    pub excerpt: String,
}

/// Aggregated retrieval evidence for one document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentEvidence {
    /// Unique document identifier
    pub document_id: String,

    /// Document filename
    pub filename: String,

    /// Full path to document
    pub file_path: String,

    /// MIME type (e.g., 'application/pdf')
    pub mime_type: String,

    /// Highest score among this document's matches
    pub max_score: f32,

    /// Number of matched excerpts/chunks
    pub match_count: usize,

    /// Matched excerpts/chunks for this document
    pub matches: Vec<DocumentEvidenceMatch>,
}

/// Output from semantic_search function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchOutput {
    /// Search results
    pub results: Vec<DocumentResult>,

    /// Document-grouped evidence packs for explainability.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documents: Vec<DocumentEvidence>,

    /// Total results matching query
    pub total_found: usize,

    /// Query execution time in milliseconds
    pub search_time_ms: f64,

    /// Original search query
    pub query: String,

    /// Where the next page of results starts, when the search filled this
    /// page and may have more.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,
}

/// Input for get_document function.
///
/// Retrieves the full content of a specific document by ID.
/// Use this after semantic_search to get complete document text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetDocumentInput {
    /// Document ID from search results
    pub document_id: String,

    /// Include file metadata (size, dates, tags)
    #[serde(default = "default_true")]
    pub include_metadata: bool,

    /// Characters to return from `offset` on. The chat's tool loop sets this
    /// to the room the result has in the model's window; without it the text
    /// runs to the end of the document.
    #[serde(default)]
    pub max_content_length: Option<usize>,

    /// Character the returned text starts at: 0 for the beginning, or a
    /// previous result's `next_offset` to read on from where it stopped.
    #[serde(default)]
    pub offset: usize,
}

fn default_true() -> bool {
    true
}

/// Document metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    pub filename: String,
    pub file_path: String,
    pub mime_type: String,
    pub extension: String,
    pub size_bytes: i64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,

    pub modified_at: DateTime<Utc>,
    pub indexed_at: DateTime<Utc>,

    #[serde(default)]
    pub tags: Vec<String>,

    pub chunk_count: usize,
}

/// Output from get_document function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetDocumentOutput {
    /// Document identifier
    pub document_id: String,

    /// Document text from `offset`, as much as was asked for.
    pub content: String,

    /// True when text after this part remains; `next_offset` is where it starts.
    pub content_truncated: bool,

    /// Character this part starts at.
    #[serde(default)]
    pub offset: usize,

    /// Character the next part starts at, when the document goes on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,

    /// Total character count for the assembled document content.
    #[serde(default)]
    pub total_chars: usize,

    /// Document metadata (if include_metadata=true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<DocumentMetadata>,
}

/// Input for list_documents function.
///
/// Browse and filter documents in the lattice.
/// Use this to explore what documents exist, filter by type/date/tags,
/// or get recent/favorite documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListDocumentsInput {
    /// Filtering mode for documents
    #[serde(default)]
    pub filter_mode: FilterMode,

    /// Filter by file extensions (for 'by_type' mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_types: Option<Vec<String>>,

    /// Filter by tags (for 'by_tag' mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,

    /// Only return documents modified after this date
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_from: Option<DateTime<Utc>>,

    /// Only return documents modified before this date
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_to: Option<DateTime<Utc>>,

    /// Maximum number of documents to return (1-500)
    #[serde(default = "default_list_limit")]
    pub limit: usize,

    /// Number of documents to skip (for pagination)
    #[serde(default)]
    pub offset: usize,

    /// Sort field
    #[serde(default)]
    pub sort_by: SortField,

    /// Sort direction
    #[serde(default)]
    pub sort_order: SortOrder,
}

fn default_list_limit() -> usize {
    50
}

/// Filtering mode for list_documents.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum FilterMode {
    #[default]
    All,
    Recent,
    Favorites,
    ByTag,
    ByType,
}

/// Sort field for list_documents.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum SortField {
    #[default]
    Modified,
    Created,
    Name,
    Size,
}

/// Sort order for list_documents.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum SortOrder {
    Asc,
    #[default]
    Desc,
}

/// Brief document information for list view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentListItem {
    pub document_id: String,
    pub filename: String,
    pub file_path: String,
    pub mime_type: String,
    pub extension: String,
    pub size_bytes: i64,
    pub modified_at: DateTime<Utc>,

    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(default)]
    pub is_favorite: bool,

    #[serde(default)]
    pub access_count: usize,
}

/// One file attached to the current conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentItem {
    /// Pass this to `get_document` to read the file.
    pub document_id: String,
    pub filename: String,
    pub extension: String,
    pub size_bytes: i64,
    pub word_count: i32,
    /// When the file was attached and indexed.
    pub indexed_at: DateTime<Utc>,
}

/// Output from list_attachments function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAttachmentsOutput {
    /// Files attached to this conversation, oldest first.
    pub attachments: Vec<AttachmentItem>,
    pub total: usize,
}

/// Output from list_documents function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListDocumentsOutput {
    /// List of documents
    pub documents: Vec<DocumentListItem>,

    /// Total documents matching filters
    pub total: usize,

    /// Requested limit
    pub limit: usize,

    /// Requested offset
    pub offset: usize,

    /// True if more results available
    pub has_more: bool,
}

/// Input for fetch_url_content function.
///
/// Fetches and extracts readable text from a web URL.
/// Use this after web_search to get full content from interesting results,
/// or when the user provides a URL to read/analyze.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchUrlContentInput {
    /// URL to fetch and extract content from
    pub url: String,
    /// Character the model wants the shown text to start at. The fetch always
    /// returns the whole page; the chat formats the part from here on that
    /// fits the result's room.
    #[serde(default)]
    pub offset: usize,
}

/// Input for wiki_search function.
///
/// Searches Wikipedia article titles and snippets via MediaWiki API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiSearchInput {
    /// Wikipedia search query.
    pub query: String,

    /// Maximum number of search results to return (1-10).
    #[serde(default = "default_max_results")]
    pub max_results: usize,
}

/// A single Wikipedia search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiSearchResult {
    /// Article title.
    pub title: String,

    /// Canonical article URL.
    pub url: String,

    /// Snippet returned by MediaWiki search.
    pub snippet: String,
}

/// Output from wiki_search function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiSearchOutput {
    /// Original query.
    pub query: String,

    /// Result count.
    pub result_count: usize,

    /// Search results.
    pub results: Vec<WikiSearchResult>,
}

/// Input for wiki_summary function.
///
/// Resolves and returns summary details for a Wikipedia page title.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiSummaryInput {
    /// Article title (e.g., "Arugula").
    pub title: String,
}

/// Output from wiki_summary function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiSummaryOutput {
    /// Canonical page title.
    pub title: String,

    /// Canonical page URL.
    pub url: String,

    /// Lead extract/summary.
    pub extract: String,

    /// Language code of returned page.
    pub language: String,
}

/// Input shape for configured custom query tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomQueryToolInput {
    /// Query text to send to the configured endpoint.
    pub query: String,

    /// Optional result limit override.
    #[serde(default = "default_max_results")]
    pub max_results: usize,
}

/// Output shape for configured custom query tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomQueryToolOutput {
    /// Tool name that handled this request.
    pub tool_name: String,

    /// Final request URL executed by the tool runtime.
    pub request_url: String,

    /// Parsed JSON payload from remote endpoint.
    pub data: serde_json::Value,
}

/// Standard error response for function calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCallError {
    /// Machine-readable error code
    pub error_code: String,

    /// Human-readable error message
    pub message: String,

    /// Additional error context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_search_input_serialization() {
        let input = SemanticSearchInput {
            query: "test query".to_string(),
            limit: 10,
            offset: 0,
            threshold: 0.5,
            search_mode: SearchMode::Hybrid,
            file_types: Some(vec!["pdf".to_string()]),
            date_from: None,
            date_to: None,
        };

        let json = serde_json::to_string(&input).unwrap();
        let deserialized: SemanticSearchInput = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.query, "test query");
        assert_eq!(deserialized.limit, 10);
    }

    #[test]
    fn test_function_call_error_serialization() {
        let error = FunctionCallError {
            error_code: "DOCUMENT_NOT_FOUND".to_string(),
            message: "Document with ID abc123 not found".to_string(),
            details: Some(serde_json::json!({"doc_id": "abc123"})),
        };

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("DOCUMENT_NOT_FOUND"));
    }
}
