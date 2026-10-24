use serde::{Deserialize, Serialize};

/// Request to ingest content from a web URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestWebUrlRequestDto {
    /// The URL to fetch and index (must be HTTP or HTTPS).
    pub url: String,
}

/// Response from ingesting a web URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestWebUrlResponseDto {
    /// Unique document ID assigned to the indexed content.
    pub document_id: String,
    /// Final URL after following redirects.
    pub url: String,
    /// Article title.
    pub title: String,
    /// Number of words in the article.
    pub word_count: usize,
    /// Number of chunks created during indexing.
    pub chunks_created: usize,
    /// Site name if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_name: Option<String>,
    /// Article author if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Estimated reading time in minutes (based on 200 WPM).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading_time_minutes: Option<i64>,
}

/// Request to get URL metadata without indexing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetUrlPreviewRequestDto {
    /// The URL to preview (must be HTTP or HTTPS).
    pub url: String,
}

/// URL metadata preview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UrlPreviewDto {
    /// Final URL after redirects.
    pub url: String,
    /// Page title.
    pub title: String,
    /// Page description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Main image URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Site name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_name: Option<String>,
}

/// Request to clean HTML article content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanArticleRequestDto {
    /// Raw HTML content to clean.
    pub html: String,
}

/// Cleaned article content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanArticleResponseDto {
    /// Clean HTML content.
    pub content: String,
    /// Extracted article title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Extracted author name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Number of words in the cleaned content.
    pub word_count: usize,
}
