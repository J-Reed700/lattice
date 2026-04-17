//! # Function Calling DTOs
//!
//! Data Transfer Objects for LLM function calling tools.
//!
//! These DTOs define the contracts for function calling interfaces that allow
//! LLMs to interact with the Recall system through structured tool use.
//!
//! ## Design Philosophy (Bricks and Studs)
//!
//! - **Minimal**: Only essential fields
//! - **Clear**: Descriptive names and comprehensive documentation
//! - **Stable**: Versioned schemas for breaking changes
//! - **Validated**: Strong typing with serde

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// =============================================================================
// Phase 1: Core Retrieval Functions
// =============================================================================

/// Input for semantic_search function.
///
/// Searches the user's document vault using semantic similarity.
/// Use this when the user asks to find, search, or retrieve information
/// from their documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchInput {
    /// Search query describing what to find
    pub query: String,

    /// Maximum number of results to return (1-50)
    #[serde(default = "default_limit")]
    pub limit: usize,

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

    /// Maximum content length in characters (prevents huge returns)
    #[serde(default = "default_max_content_length")]
    pub max_content_length: usize,

    /// 1-based page number for internal pagination.
    ///
    /// Page size is controlled by `max_content_length`.
    #[serde(default = "default_page")]
    pub page: usize,
}

fn default_true() -> bool {
    true
}

fn default_max_content_length() -> usize {
    50000
}

fn default_page() -> usize {
    1
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

    /// Document text content for the requested page
    pub content: String,

    /// True if additional pages are available beyond the returned page
    pub content_truncated: bool,

    /// 1-based page returned.
    #[serde(default = "default_page")]
    pub page: usize,

    /// Total available pages for this document at the requested page size.
    #[serde(default = "default_page")]
    pub total_pages: usize,

    /// Total character count for the assembled document content.
    #[serde(default)]
    pub total_chars: usize,

    /// True when a previous page exists.
    #[serde(default)]
    pub has_previous_page: bool,

    /// True when a next page exists.
    #[serde(default)]
    pub has_next_page: bool,

    /// Previous page number if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_page: Option<usize>,

    /// Next page number if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page: Option<usize>,

    /// Document metadata (if include_metadata=true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<DocumentMetadata>,
}

/// Input for list_documents function.
///
/// Browse and filter documents in the vault.
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

// =============================================================================
// Phase 2: Web Integration Functions
// =============================================================================

/// Input for web_search function.
///
/// Searches the web using DuckDuckGo when information isn't in the vault.
/// Use this when the user asks about current events, external information,
/// or when vault search returns no results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchInput {
    /// Web search query
    pub query: String,

    /// Maximum number of web results to return for this page (1-50)
    #[serde(default = "default_max_results")]
    pub max_results: usize,

    /// 1-based page number for paginated web search.
    #[serde(default = "default_web_page")]
    pub page: usize,

    /// Absolute result offset for pagination (applied before page).
    #[serde(default)]
    pub offset: usize,

    /// Preferred provider order (duckduckgo, bing, wikipedia).
    #[serde(default)]
    pub providers: Vec<String>,

    /// Include Wikipedia enrichment in search results.
    #[serde(default)]
    pub include_wikipedia: bool,

    /// Recursive deep-research depth (1-4).
    #[serde(default = "default_research_depth")]
    pub depth: usize,

    /// Maximum follow-up query branches per recursion step (1-4).
    #[serde(default = "default_research_branch_queries")]
    pub branch_queries: usize,
}

fn default_max_results() -> usize {
    5
}

fn default_web_page() -> usize {
    1
}

fn default_research_depth() -> usize {
    1
}

fn default_research_branch_queries() -> usize {
    2
}

/// A single web search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchResult {
    /// Page title
    pub title: String,

    /// Page URL
    pub url: String,

    /// Text snippet from the page
    pub snippet: String,

    /// Publication date if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_date: Option<DateTime<Utc>>,

    /// Source provider that produced this result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Output from web_search function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchOutput {
    /// Web search results
    pub results: Vec<WebSearchResult>,

    /// Original search query
    pub query: String,

    /// Number of results returned
    pub result_count: usize,

    /// 1-based page returned.
    #[serde(default = "default_web_page")]
    pub page: usize,

    /// Effective offset used before pagination.
    #[serde(default)]
    pub offset: usize,

    /// Total deduplicated results discovered across providers.
    #[serde(default)]
    pub total_results: usize,

    /// True if additional results are likely available.
    #[serde(default)]
    pub has_more: bool,

    /// Providers that contributed results.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers_used: Vec<String>,

    /// Number of unique normalized queries executed across deep-research recursion.
    #[serde(default)]
    pub unique_query_count: usize,

    /// Number of unique canonical URLs discovered before pagination.
    #[serde(default)]
    pub unique_url_count: usize,

    /// Number of unique domains discovered before pagination.
    #[serde(default)]
    pub unique_domain_count: usize,
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
}

/// Output from fetch_url_content function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchUrlContentOutput {
    /// Fetched URL (may differ if redirected)
    pub url: String,

    /// Page title if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Extracted text content
    pub content: String,

    /// True if content exceeded internal extraction limits
    pub content_truncated: bool,

    /// Word count of extracted content
    pub word_count: usize,

    /// Fetch time in milliseconds
    pub fetch_time_ms: f64,

    /// HTTP Content-Type header
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
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

/// URL preview information for web capture.
///
/// Contains metadata about a web page for preview before ingestion.
/// Includes OpenGraph, Schema.org, and general metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlPreview {
    /// Page URL (after redirects)
    pub url: String,

    /// Page title (from `<title>`, og:title, or first `<h1>`)
    pub title: String,

    /// Page description (from meta description or og:description)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Site name (from og:site_name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_name: Option<String>,

    /// Main image URL (from og:image or first article image)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,

    /// Author name (from meta author, Schema.org, or article byline)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// Publication date (from Schema.org or article metadata)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_date: Option<DateTime<Utc>>,

    /// Estimated word count
    pub word_count: usize,

    /// Estimated reading time in minutes
    pub reading_time_minutes: i64,

    /// Page language (from `<html lang>` or meta)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Content type (e.g., "article", "website", "video")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,

    /// Keywords/tags extracted from meta keywords
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub keywords: Vec<String>,
}

/// Clean article content extracted from HTML (reader mode).
///
/// Contains the main article content with boilerplate removed.
/// Similar to Firefox Reader Mode or Pocket's article view.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanArticle {
    /// Article title
    pub title: String,

    /// Article author (from meta tags or byline)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// Clean HTML content (ads, navigation, sidebars removed)
    pub content: String,

    /// Plain text content (no HTML tags)
    pub text_content: String,

    /// Word count of article text
    pub word_count: usize,

    /// Estimated reading time in minutes (200 words per minute)
    pub reading_time_minutes: i64,

    /// Publication date (from meta tags or Schema.org)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_date: Option<DateTime<Utc>>,

    /// Excerpt (first ~200 characters of text)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
}

// =============================================================================
// Common Error Response
// =============================================================================

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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_search_input_serialization() {
        let input = SemanticSearchInput {
            query: "test query".to_string(),
            limit: 10,
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
    fn test_web_search_input_defaults() {
        let input = WebSearchInput {
            query: "test".to_string(),
            max_results: default_max_results(),
            page: default_web_page(),
            offset: 0,
            providers: Vec::new(),
            include_wikipedia: false,
            depth: default_research_depth(),
            branch_queries: default_research_branch_queries(),
        };

        assert_eq!(input.max_results, 5);
        assert_eq!(input.page, 1);
        assert_eq!(input.depth, 1);
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
