//! Web capture service trait
//!
//! Provides URL preview and metadata extraction for web content ingestion.

use crate::features::function_calling::dto::UrlPreview;
use crate::shared::error::Result;
use async_trait::async_trait;

/// Trait for web capture operations
///
/// Provides URL preview functionality including:
/// - Full HTML download
/// - Article content extraction (reader mode)
/// - OpenGraph metadata extraction
/// - Schema.org JSON-LD parsing
/// - General metadata extraction (title, description, author, etc.)
///
/// # Security
///
/// Implementations MUST:
/// - Validate URLs to prevent SSRF (CWE-918)
/// - Block private IP ranges and localhost
/// - Apply request timeouts
/// - Limit content size to prevent memory exhaustion
///
/// # Implementations
/// - `WebCaptureService`: Production implementation with reqwest and scraper
/// - `MockWebCaptureService`: In-memory mock for testing
#[async_trait]
pub trait WebCaptureServiceTrait: Send + Sync {
    /// Fetch URL preview with metadata extraction
    ///
    /// Downloads the webpage HTML and extracts comprehensive metadata including:
    /// - Basic metadata: title, description, language
    /// - OpenGraph tags: og:title, og:description, og:image, og:site_name, og:type
    /// - Schema.org JSON-LD: author, publisher, datePublished
    /// - Article metadata: author, published date, word count, reading time
    ///
    /// # Arguments
    /// * `url` - The URL to preview (must be valid HTTP/HTTPS URL)
    ///
    /// # Returns
    /// `UrlPreview` containing extracted metadata and article information
    ///
    /// # Errors
    /// - `AppError::InvalidUrl` if URL validation fails (SSRF prevention)
    /// - `AppError::Network` if HTTP request fails or times out
    /// - `AppError::Other` if HTML parsing or metadata extraction fails
    ///
    /// # Example
    /// ```rust,no_run
    /// use lattice::infrastructure::services::traits::WebCaptureServiceTrait;
    ///
    /// # async fn example<S: WebCaptureServiceTrait>(service: &S) -> Result<(), Box<dyn std::error::Error>> {
    /// let preview = service.fetch_url_preview("https://example.com/article").await?;
    /// println!("Title: {}", preview.title);
    /// println!("Author: {:?}", preview.author);
    /// println!("Reading time: {} minutes", preview.reading_time_minutes);
    /// # Ok(())
    /// # }
    /// ```
    async fn fetch_url_preview(&self, url: &str) -> Result<UrlPreview>;
}
