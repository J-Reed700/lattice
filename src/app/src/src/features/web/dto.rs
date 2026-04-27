//! # Web Ingestion DTOs
//!
//! Data Transfer Objects for web content ingestion operations.
//!
//! These DTOs define the contracts for:
//! - URL ingestion (fetch + index)
//! - URL preview (metadata extraction without indexing)
//! - Article content cleaning
//!
//! ## Design
//!
//! - **Minimal**: Only essential fields for web operations
//! - **Clear**: Descriptive names following REST conventions
//! - **Stable**: Versioned for API compatibility
//! - **Validated**: Strong typing with serde

use serde::{Deserialize, Serialize};

// =============================================================================
// Ingest Web URL
// =============================================================================

/// Request to ingest content from a web URL.
///
/// Fetches the URL, extracts article content, and indexes it into the lattice.
///
/// # Example
/// ```rust
/// use vault_desktop::application::dtos::web_dto::IngestWebUrlRequestDto;
///
/// let request = IngestWebUrlRequestDto {
///     url: "https://example.com/article".to_string(),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestWebUrlRequestDto {
    /// The URL to fetch and index (must be HTTP or HTTPS)
    pub url: String,
}

/// Response from ingesting a web URL.
///
/// Contains metadata about the indexed document.
///
/// # Example
/// ```rust
/// # use vault_desktop::application::dtos::web_dto::IngestWebUrlResponseDto;
/// let response = IngestWebUrlResponseDto {
///     document_id: "doc-123".to_string(),
///     url: "https://example.com/article".to_string(),
///     title: "Article Title".to_string(),
///     word_count: 1500,
///     chunks_created: 3,
///     site_name: Some("Example Site".to_string()),
///     author: Some("John Doe".to_string()),
///     reading_time_minutes: Some(7),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestWebUrlResponseDto {
    /// Unique document ID assigned to the indexed content
    pub document_id: String,

    /// Final URL after following redirects
    pub url: String,

    /// Article title
    pub title: String,

    /// Number of words in the article
    pub word_count: usize,

    /// Number of chunks created during indexing
    pub chunks_created: usize,

    /// Site name if available (e.g., "Medium", "New York Times")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_name: Option<String>,

    /// Article author if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// Estimated reading time in minutes (based on 200 WPM)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading_time_minutes: Option<i64>,
}

// =============================================================================
// Get URL Preview
// =============================================================================

/// Request to get URL metadata without indexing.
///
/// Fetches metadata (title, description, image) without actually
/// ingesting the content into the lattice.
///
/// # Example
/// ```rust
/// use vault_desktop::application::dtos::web_dto::GetUrlPreviewRequestDto;
///
/// let request = GetUrlPreviewRequestDto {
///     url: "https://example.com/article".to_string(),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetUrlPreviewRequestDto {
    /// The URL to preview (must be HTTP or HTTPS)
    pub url: String,
}

/// URL metadata preview.
///
/// Contains Open Graph / Twitter Card metadata extracted from the URL.
///
/// # Example
/// ```rust
/// # use vault_desktop::application::dtos::web_dto::UrlPreviewDto;
/// let preview = UrlPreviewDto {
///     url: "https://example.com/article".to_string(),
///     title: "Article Title".to_string(),
///     description: Some("Article description...".to_string()),
///     image: Some("https://example.com/image.jpg".to_string()),
///     site_name: Some("Example Site".to_string()),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlPreviewDto {
    /// Final URL (after redirects)
    pub url: String,

    /// Page title (from `<title>`, og:title, or first `<h1>`)
    pub title: String,

    /// Page description (from meta description or og:description)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Main image URL (from og:image or first article image)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,

    /// Site name (from og:site_name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_name: Option<String>,
}

// =============================================================================
// Clean Article Content
// =============================================================================

/// Request to clean HTML article content.
///
/// Applies readability algorithm to extract clean article content
/// from raw HTML (removes ads, navigation, boilerplate).
///
/// # Example
/// ```rust
/// use vault_desktop::application::dtos::web_dto::CleanArticleRequestDto;
///
/// let request = CleanArticleRequestDto {
///     html: "<html><body><article>...</article></body></html>".to_string(),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanArticleRequestDto {
    /// Raw HTML content to clean
    pub html: String,
}

/// Cleaned article content.
///
/// Contains the extracted article with boilerplate removed.
///
/// # Example
/// ```rust
/// # use vault_desktop::application::dtos::web_dto::CleanArticleResponseDto;
/// let cleaned = CleanArticleResponseDto {
///     content: "<p>Clean article content...</p>".to_string(),
///     title: Some("Article Title".to_string()),
///     author: Some("John Doe".to_string()),
///     word_count: 1500,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanArticleResponseDto {
    /// Clean HTML content (ads, navigation removed)
    pub content: String,

    /// Extracted article title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Extracted author name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,

    /// Number of words in the cleaned content
    pub word_count: usize,
}
