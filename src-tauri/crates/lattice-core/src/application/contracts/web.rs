//! Web records exchanged between the web service and the tools, chat turns
//! and lessons that call it: search and fetch results, page previews and
//! reader-mode articles.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The version stamped on every reference text the web reader extracts for a
/// learning collection. Bump it when extraction changes what a stored
/// reference would contain.
pub const REFERENCE_TEXT_EXTRACTION_VERSION: &str = "web_reference_markdown_v2";

/// Input for web_search function.
///
/// Searches the web using DuckDuckGo when information isn't in the lattice.
/// Use this when the user asks about current events, external information,
/// or when lattice search returns no results.
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

    /// Searches to run after `query`, `branch_queries` per level of `depth`
    /// past the first. Each should take a different angle on the same
    /// subject. Without them a deep search is just `query`.
    #[serde(default)]
    pub followup_queries: Vec<String>,
}

/// Results per page when a search tool's input leaves it out; the wiki and
/// custom-query tool inputs share it.
pub fn default_max_results() -> usize {
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

    /// Every query a deep search ran beyond the one it was asked, in the order
    /// it ran them. They used to reach the log and nowhere else, so a reader
    /// saw one search where there had been six and could not tell what the
    /// results were actually results *for*.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub followup_queries: Vec<String>,
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

    /// True when the page came from the on-disk page cache instead of the
    /// network. Left out of the payload entirely for a live fetch, so what a
    /// model sees for an ordinary page is exactly what it saw before.
    #[serde(default, skip_serializing_if = "is_false")]
    pub from_cache: bool,
}

/// `skip_serializing_if` for a flag only worth sending when it is set.
fn is_false(value: &bool) -> bool {
    !*value
}

/// URL preview information for web capture.
///
/// Contains metadata about a web page for preview before ingestion.
/// Includes OpenGraph, Schema.org, and general metadata.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
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

#[cfg(test)]
mod tests {
    use super::*;

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
            followup_queries: Vec::new(),
        };

        assert_eq!(input.max_results, 5);
        assert_eq!(input.page, 1);
        assert_eq!(input.depth, 1);
    }
}
