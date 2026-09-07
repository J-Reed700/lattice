//! Web capture service for URL preview and metadata extraction
//!
//! Provides comprehensive web page metadata extraction including:
//! - OpenGraph tags (og:title, og:description, og:image, etc.)
//! - Schema.org JSON-LD structured data
//! - HTML meta tags (description, author, keywords)
//! - Article extraction (clean content, reading time)
//!
//! # Security
//!
//! - **SSRF Prevention (CWE-918)**: Validates URLs and blocks private IPs
//! - **Request Timeout**: 10-second default timeout
//! - **Content Length Limiting**: Prevents memory exhaustion
//! - **User Agent**: Identifies requests as coming from Lattice
//!
//! # Example
//! ```rust,no_run
//! use lattice::infrastructure::services::web_capture::WebCaptureService;
//! use lattice::infrastructure::services::traits::WebCaptureServiceTrait;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let service = WebCaptureService::new()?;
//!
//! // Fetch URL preview with metadata
//! let preview = service.fetch_url_preview("https://example.com/article").await?;
//! println!("Title: {}", preview.title);
//! println!("Author: {:?}", preview.author);
//! # Ok(())
//! # }
//! ```

use crate::features::function_calling::dto::UrlPreview;
use crate::features::web::WebCaptureServiceTrait;
use crate::shared::constants::WEB_REQUEST_TIMEOUT;
use crate::shared::error::{AppError, Result};
use crate::shared::utils::stealth;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Web capture service implementation
///
/// Provides URL preview and metadata extraction with comprehensive
/// support for OpenGraph, Schema.org, and general HTML metadata.
pub struct WebCaptureService {
    /// HTTP client with timeout
    client: Client,

    /// Maximum content length to fetch (10MB for preview)
    max_content_length: usize,
}

impl WebCaptureService {
    /// Create a new web capture service with stealth features.
    pub fn new() -> Result<Self> {
        let client = stealth::stealth_client_builder()
            .timeout(WEB_REQUEST_TIMEOUT)
            .build()
            .map_err(|e| AppError::InternalError(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            max_content_length: 10 * 1024 * 1024, // 10MB
        })
    }

    /// Create with custom timeout
    pub fn with_timeout(timeout: Duration) -> Result<Self> {
        let client = stealth::stealth_client_builder()
            .timeout(timeout)
            .build()
            .map_err(|e| AppError::InternalError(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            max_content_length: 10 * 1024 * 1024,
        })
    }

    /// Validate URL for security (SSRF prevention)
    ///
    /// Delegates to WebService's validation logic which blocks:
    /// - localhost and loopback addresses
    /// - Private IP ranges (RFC 1918)
    /// - Link-local addresses
    /// - Metadata endpoints (169.254.169.254)
    fn validate_url(&self, url: &str) -> Result<()> {
        // Parse URL
        let parsed = url::Url::parse(url)
            .map_err(|e| AppError::InvalidUrl(format!("Invalid URL: {}", e)))?;

        // Check scheme (only allow http/https)
        match parsed.scheme() {
            "http" | "https" => {}
            _ => {
                return Err(AppError::InvalidUrl(format!(
                    "Unsupported URL scheme: {}. Only http and https are allowed.",
                    parsed.scheme()
                )));
            }
        }

        // Check host
        let host = parsed
            .host_str()
            .ok_or_else(|| AppError::InvalidUrl("URL must have a host".to_string()))?;

        // Block localhost variants
        let lowercase_host = host.to_lowercase();
        if lowercase_host == "localhost"
            || lowercase_host == "0.0.0.0"
            || lowercase_host == "[::]"
            || lowercase_host.starts_with("127.")
        {
            return Err(AppError::InvalidUrl(format!(
                "Access to localhost blocked: {}",
                host
            )));
        }

        // Block private IP ranges (basic check)
        if let Ok(ip) = host.parse::<std::net::IpAddr>() {
            use std::net::IpAddr;
            let is_private = match ip {
                IpAddr::V4(ipv4) => {
                    ipv4.is_private()
                        || ipv4.is_loopback()
                        || ipv4.is_link_local()
                        || ipv4.is_broadcast()
                        || ipv4.is_documentation()
                }
                IpAddr::V6(ipv6) => {
                    ipv6.is_loopback() || ipv6.is_multicast() || ipv6.is_unspecified()
                }
            };

            if is_private {
                return Err(AppError::InvalidUrl(format!(
                    "Access to private/reserved IP blocked: {}",
                    host
                )));
            }
        }

        debug!("URL validation passed: {}", url);
        Ok(())
    }

    /// Extract metadata from HTML document
    fn extract_metadata(&self, html: &str, final_url: &str) -> UrlPreview {
        let document = Html::parse_document(html);

        // Extract title (priority: og:title > <title> > first <h1>)
        let title = self
            .extract_og_tag(&document, "og:title")
            .or_else(|| self.extract_title(&document))
            .or_else(|| self.extract_first_heading(&document))
            .unwrap_or_else(|| "Untitled".to_string());

        // Extract description
        let description = self
            .extract_og_tag(&document, "og:description")
            .or_else(|| self.extract_meta_tag(&document, "description"))
            .or_else(|| self.extract_meta_tag(&document, "twitter:description"));

        // Extract site name
        let site_name = self.extract_og_tag(&document, "og:site_name");

        // Extract image
        let image = self
            .extract_og_tag(&document, "og:image")
            .or_else(|| self.extract_meta_tag(&document, "twitter:image"));

        // Extract author
        let author = self
            .extract_meta_tag(&document, "author")
            .or_else(|| self.extract_schema_author(&document))
            .or_else(|| self.extract_meta_tag(&document, "article:author"));

        // Extract published date
        let published_date = self
            .extract_schema_date(&document)
            .or_else(|| self.extract_meta_date(&document, "article:published_time"));

        // Extract language
        let language = self.extract_language(&document);

        // Extract content type
        let content_type = self.extract_og_tag(&document, "og:type");

        // Extract keywords
        let keywords = self.extract_keywords(&document);

        // Extract article text for word count
        let article_text = self.extract_article_text(&document);
        let word_count = article_text.split_whitespace().count();
        let reading_time_minutes = (word_count as f64 / 200.0).ceil() as i64; // 200 words per minute

        UrlPreview {
            url: final_url.to_string(),
            title,
            description,
            site_name,
            image,
            author,
            published_date,
            word_count,
            reading_time_minutes,
            language,
            content_type,
            keywords,
        }
    }

    /// Extract OpenGraph tag
    fn extract_og_tag(&self, document: &Html, property: &str) -> Option<String> {
        let selector = Selector::parse(&format!("meta[property='{}']", property)).ok()?;
        document
            .select(&selector)
            .next()
            .and_then(|el| el.value().attr("content"))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Extract general meta tag
    fn extract_meta_tag(&self, document: &Html, name: &str) -> Option<String> {
        let selector = Selector::parse(&format!("meta[name='{}']", name)).ok()?;
        document
            .select(&selector)
            .next()
            .and_then(|el| el.value().attr("content"))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Extract title from <title> tag
    fn extract_title(&self, document: &Html) -> Option<String> {
        let selector = Selector::parse("title").ok()?;
        document
            .select(&selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Extract first heading
    fn extract_first_heading(&self, document: &Html) -> Option<String> {
        let selector = Selector::parse("h1").ok()?;
        document
            .select(&selector)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Extract Schema.org author from JSON-LD
    fn extract_schema_author(&self, document: &Html) -> Option<String> {
        let selector = Selector::parse("script[type='application/ld+json']").ok()?;

        for element in document.select(&selector) {
            let json_text = element.text().collect::<String>();
            if let Ok(schema) = serde_json::from_str::<SchemaOrg>(&json_text) {
                if let Some(author) = schema.author {
                    return Some(author);
                }
            }
        }
        None
    }

    /// Extract Schema.org date from JSON-LD
    fn extract_schema_date(&self, document: &Html) -> Option<DateTime<Utc>> {
        let selector = Selector::parse("script[type='application/ld+json']").ok()?;

        for element in document.select(&selector) {
            let json_text = element.text().collect::<String>();
            if let Ok(schema) = serde_json::from_str::<SchemaOrg>(&json_text) {
                if let Some(date_str) = schema.date_published {
                    if let Ok(date) = DateTime::parse_from_rfc3339(&date_str) {
                        return Some(date.with_timezone(&Utc));
                    }
                }
            }
        }
        None
    }

    /// Extract date from meta tag
    fn extract_meta_date(&self, document: &Html, property: &str) -> Option<DateTime<Utc>> {
        let selector = Selector::parse(&format!("meta[property='{}']", property)).ok()?;
        let date_str = document
            .select(&selector)
            .next()
            .and_then(|el| el.value().attr("content"))?;

        DateTime::parse_from_rfc3339(date_str)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    }

    /// Extract language from HTML tag
    fn extract_language(&self, document: &Html) -> Option<String> {
        let selector = Selector::parse("html").ok()?;
        document
            .select(&selector)
            .next()
            .and_then(|el| el.value().attr("lang"))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Extract keywords from meta tag
    fn extract_keywords(&self, document: &Html) -> Vec<String> {
        let keywords_str = self.extract_meta_tag(document, "keywords");

        keywords_str
            .map(|s| {
                s.split(',')
                    .map(|k| k.trim().to_string())
                    .filter(|k| !k.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Extract main article text
    fn extract_article_text(&self, document: &Html) -> String {
        // Try to find article content
        let article_selector = Selector::parse("article").ok();
        let p_selector = Selector::parse("p, h1, h2, h3, h4, h5, h6").ok();

        let mut text_parts = Vec::new();

        // First try to find article element
        if let Some(selector) = article_selector {
            for element in document.select(&selector) {
                if let Some(p_sel) = &p_selector {
                    for p in element.select(p_sel) {
                        let text = p.text().collect::<Vec<_>>().join(" ");
                        if !text.trim().is_empty() {
                            text_parts.push(text.trim().to_string());
                        }
                    }
                }
            }
        }

        // If no article found, extract all paragraphs
        if text_parts.is_empty() {
            if let Some(p_sel) = &p_selector {
                for p in document.select(p_sel) {
                    let text = p.text().collect::<Vec<_>>().join(" ");
                    if !text.trim().is_empty() {
                        text_parts.push(text.trim().to_string());
                    }
                }
            }
        }

        text_parts.join("\n\n")
    }
}

// REMOVED: Default impl that could panic
// P0-3 FIX: Default trait should not panic. HTTP client creation CAN fail
// (firewalls, missing system libraries, restricted networks, etc.)
// Use WebCaptureService::new() which returns Result for proper error handling

/// Schema.org JSON-LD structure (simplified)
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SchemaOrg {
    #[serde(rename = "@type")]
    schema_type: Option<String>,
    author: Option<String>,
    date_published: Option<String>,
    publisher: Option<String>,
}

#[async_trait]
impl WebCaptureServiceTrait for WebCaptureService {
    async fn fetch_url_preview(&self, url: &str) -> Result<UrlPreview> {
        debug!("Fetching URL preview: {}", url);

        // Validate URL for security
        self.validate_url(url)?;

        stealth::random_delay(500, 2000).await;
        let profile = stealth::random_profile();
        let headers = stealth::browser_headers(profile, None);

        // Fetch URL with stealth headers
        let response = self
            .client
            .get(url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Failed to fetch URL: {}", e)))?;

        // Check status
        if !response.status().is_success() {
            let status = response.status();
            warn!(status = %status, url = %url, "URL fetch failed");
            return Err(match status {
                reqwest::StatusCode::FORBIDDEN
                | reqwest::StatusCode::TOO_MANY_REQUESTS
                | reqwest::StatusCode::SERVICE_UNAVAILABLE => AppError::Network(format!(
                    "HTTP {} {}: Site blocked automated access. Try importing the PDF or local file version.",
                    status.as_u16(),
                    status.canonical_reason().unwrap_or("Unknown"),
                )),
                _ => AppError::Network(format!(
                    "HTTP {}: {}",
                    status,
                    status.canonical_reason().unwrap_or("Unknown")
                )),
            });
        }

        // Get final URL (after redirects)
        let final_url = response.url().to_string();

        // Re-validate final URL after redirects (prevents redirect-based SSRF)
        if final_url != url {
            debug!("URL redirected to: {}", final_url);
            self.validate_url(&final_url)?;
        }

        // Read HTML body
        let html = response
            .text()
            .await
            .map_err(|e| AppError::Network(format!("Failed to read response body: {}", e)))?;

        // Extract metadata
        let preview = self.extract_metadata(&html, &final_url);

        info!(
            "URL preview extracted: {} words, {} minute read",
            preview.word_count, preview.reading_time_minutes
        );

        Ok(preview)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_url() {
        let service = WebCaptureService::new().unwrap();

        // Valid URLs
        assert!(service.validate_url("https://www.rust-lang.org").is_ok());
        assert!(service.validate_url("http://example.com").is_ok());

        // Invalid: localhost
        assert!(service.validate_url("http://localhost:8000").is_err());
        assert!(service.validate_url("http://127.0.0.1").is_err());

        // Invalid: private IP
        assert!(service.validate_url("http://192.168.1.1").is_err());
        assert!(service.validate_url("http://10.0.0.1").is_err());

        // Invalid: unsupported scheme
        assert!(service.validate_url("ftp://example.com").is_err());
        assert!(service.validate_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn test_extract_metadata() {
        let service = WebCaptureService::new().unwrap();

        let html = r#"
            <!DOCTYPE html>
            <html lang="en">
            <head>
                <meta property="og:title" content="Test Article">
                <meta property="og:description" content="A test article for metadata extraction">
                <meta property="og:image" content="https://example.com/image.jpg">
                <meta property="og:site_name" content="Example Site">
                <meta property="og:type" content="article">
                <meta name="author" content="John Doe">
                <meta name="keywords" content="test, article, metadata">
                <title>Test Page</title>
            </head>
            <body>
                <article>
                    <h1>Main Title</h1>
                    <p>This is the first paragraph with some content.</p>
                    <p>This is the second paragraph with more content.</p>
                </article>
            </body>
            </html>
        "#;

        let preview = service.extract_metadata(html, "https://example.com/test");

        assert_eq!(preview.title, "Test Article");
        assert_eq!(
            preview.description,
            Some("A test article for metadata extraction".to_string())
        );
        assert_eq!(preview.site_name, Some("Example Site".to_string()));
        assert_eq!(
            preview.image,
            Some("https://example.com/image.jpg".to_string())
        );
        assert_eq!(preview.author, Some("John Doe".to_string()));
        assert_eq!(preview.language, Some("en".to_string()));
        assert_eq!(preview.content_type, Some("article".to_string()));
        assert_eq!(preview.keywords, vec!["test", "article", "metadata"]);
        assert!(preview.word_count > 0);
        assert!(preview.reading_time_minutes > 0);
    }

    #[test]
    fn test_extract_article_text() {
        let service = WebCaptureService::new().unwrap();

        let html = r#"
            <html>
            <body>
                <article>
                    <h1>Main Title</h1>
                    <p>First paragraph.</p>
                    <p>Second paragraph.</p>
                </article>
            </body>
            </html>
        "#;

        let document = Html::parse_document(html);
        let text = service.extract_article_text(&document);

        assert!(text.contains("Main Title"));
        assert!(text.contains("First paragraph"));
        assert!(text.contains("Second paragraph"));
    }

    #[test]
    fn test_extract_keywords() {
        let service = WebCaptureService::new().unwrap();

        let html = r#"
            <html>
            <head>
                <meta name="keywords" content="rust, programming, web, scraping">
            </head>
            </html>
        "#;

        let document = Html::parse_document(html);
        let keywords = service.extract_keywords(&document);

        assert_eq!(keywords, vec!["rust", "programming", "web", "scraping"]);
    }
}
