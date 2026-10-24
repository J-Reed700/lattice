//! Data Transfer Objects for LLM function calling tools.

use crate::application::contracts::web::default_max_results;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Input for semantic_search function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchInput {
    pub query: String,

    #[serde(default = "default_limit")]
    pub limit: usize,

    /// Results to skip: 0, or the `next_offset` a previous search named.
    #[serde(default)]
    pub offset: usize,

    /// Minimum similarity score (0.0-1.0). Lower = more results.
    #[serde(default = "default_threshold")]
    pub threshold: f32,

    #[serde(default)]
    pub search_mode: SearchMode,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_types: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_from: Option<DateTime<Utc>>,

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
    Semantic,
    Keyword,
    #[default]
    Hybrid,
}

/// A single document search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentResult {
    pub document_id: String,
    pub filename: String,
    pub file_path: String,
    pub mime_type: String,
    pub score: f32,
    pub snippet: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_index: Option<usize>,

    pub modified_at: DateTime<Utc>,
    pub size_bytes: i64,
}

/// A single matched excerpt within a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentEvidenceMatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_index: Option<usize>,
    pub score: f32,
    pub excerpt: String,
}

/// Aggregated retrieval evidence for one document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentEvidence {
    pub document_id: String,
    pub filename: String,
    pub file_path: String,
    pub mime_type: String,
    pub max_score: f32,
    pub match_count: usize,
    pub matches: Vec<DocumentEvidenceMatch>,
}

/// Output from semantic_search function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchOutput {
    pub results: Vec<DocumentResult>,

    /// Document-grouped evidence packs for explainability.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documents: Vec<DocumentEvidence>,

    pub total_found: usize,
    pub search_time_ms: f64,
    pub query: String,

    /// Where the next page of results starts, when the search filled this page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,
}

/// Input for get_document function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetDocumentInput {
    pub document_id: String,

    #[serde(default = "default_true")]
    pub include_metadata: bool,

    /// Characters to return from `offset` on.
    #[serde(default)]
    pub max_content_length: Option<usize>,

    /// Character the returned text starts at.
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
    /// Document metadata (if include_metadata=true).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<DocumentMetadata>,
}

/// Input for list_documents function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListDocumentsInput {
    #[serde(default)]
    pub filter_mode: FilterMode,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_types: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_from: Option<DateTime<Utc>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_to: Option<DateTime<Utc>>,

    #[serde(default = "default_list_limit")]
    pub limit: usize,

    #[serde(default)]
    pub offset: usize,

    #[serde(default)]
    pub sort_by: SortField,

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
    pub document_id: String,
    pub filename: String,
    pub extension: String,
    pub size_bytes: i64,
    pub word_count: i32,
    pub indexed_at: DateTime<Utc>,
}

/// Output from list_attachments function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAttachmentsOutput {
    pub attachments: Vec<AttachmentItem>,
    pub total: usize,
}

/// Output from list_documents function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListDocumentsOutput {
    pub documents: Vec<DocumentListItem>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub has_more: bool,
}

/// Input for fetch_url_content function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchUrlContentInput {
    pub url: String,
    /// Character the model wants the shown text to start at.
    #[serde(default)]
    pub offset: usize,
    /// Optional ISO date (`YYYY-MM-DD`) to read from archive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<String>,
}

/// Input for `find_sources` tool (design §3.11).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindSourcesInput {
    /// Primary-source category (`official_docs`, `package`, `paper`,
    /// `standard`, `law`, `community`, `encyclopedia`).
    pub kind: String,
    pub query: String,
    pub subject: Option<String>,
    pub identifier: Option<String>,
    /// The registry a package is published in (`pypi`, `crates`, `npm`,
    /// `nuget`, `rubygems`, `maven`, `go`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ecosystem: Option<String>,
    pub site: Option<String>,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
    #[serde(default = "default_find_sources_page")]
    pub page: usize,
}

fn default_find_sources_page() -> usize {
    1
}

/// Input for `follow_links` tool (design §3.11).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowLinksInput {
    pub url: String,
    /// Optional quote from the page; links in and around its paragraph are returned first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub near_text: Option<String>,
}

/// One link extracted by `follow_links`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowedLinkDto {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

/// Output from `follow_links`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowLinksOutput {
    pub url: String,
    pub links: Vec<FollowedLinkDto>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outcomes: Vec<crate::application::contracts::web::FinderOutcomeDto>,
}

/// Input for wiki_search function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiSearchInput {
    pub query: String,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
}

/// A single Wikipedia search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiSearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Output from wiki_search function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiSearchOutput {
    pub query: String,
    pub result_count: usize,
    pub results: Vec<WikiSearchResult>,
}

/// Input for wiki_summary function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiSummaryInput {
    pub title: String,
}

/// Output from wiki_summary function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiSummaryOutput {
    pub title: String,
    pub url: String,
    pub extract: String,
    pub language: String,
}

/// Input shape for configured custom query tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomQueryToolInput {
    pub query: String,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
}

/// Output shape for configured custom query tools.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomQueryToolOutput {
    pub tool_name: String,
    pub request_url: String,
    pub data: serde_json::Value,
}

/// Standard error response for function calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCallError {
    pub error_code: String,
    pub message: String,
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
