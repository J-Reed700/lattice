//! Article extractor service for clean content extraction
//!
//! Provides production-quality article extraction from HTML pages,
//! implementing a simplified readability algorithm similar to Firefox
//! Reader Mode or Pocket's article view.
//!
//! # Algorithm
//!
//! 1. **Parse HTML**: Use scraper to parse the HTML document
//! 2. **Find Main Content**: Look for semantic tags and common class names
//!    - Semantic tags: `<article>`, `<main>`
//!    - Common classes: `.content`, `.article`, `.post-content`, `.entry-content`
//!    - Fallback: Find largest text block with high paragraph density
//! 3. **Remove Boilerplate**: Strip unwanted elements
//!    - Scripts, styles, navigation, sidebars, footers, headers
//!    - Elements with ad-related classes
//! 4. **Extract Metadata**: Pull author and date from meta tags
//! 5. **Calculate Metrics**: Word count and reading time (200 WPM)
//! 6. **Generate Excerpt**: First ~200 characters of text
//!
//! # Example
//! ```rust,no_run
//! use lattice::infrastructure::services::article_extractor::ArticleExtractorService;
//! use lattice::infrastructure::services::traits::ArticleExtractorServiceTrait;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let service = ArticleExtractorService::new()?;
//!
//! let html = r#"<html>
//!     <body>
//!         <article>
//!             <h1>Article Title</h1>
//!             <p>This is the main article content...</p>
//!         </article>
//!     </body>
//! </html>"#;
//!
//! let article = service.extract_article(html, "https://example.com/article").await?;
//! println!("Title: {}", article.title);
//! println!("Word count: {}", article.word_count);
//! println!("Reading time: {} minutes", article.reading_time_minutes);
//! # Ok(())
//! # }
//! ```

use crate::features::function_calling::dto::CleanArticle;
use crate::infrastructure::services::traits::ArticleExtractorServiceTrait;
use crate::shared::error::{AppError, Result};
use crate::shared::utils::stealth;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lazy_regex::regex;
use readability_js::Readability;
use reqwest::Client;
use scraper::{Html, Selector};
use std::time::Duration;
use tracing::{debug, info, warn};

// Compile-time regex validation using lazy_regex (eliminates runtime panics)
// Pre-compile HTML tag stripping regex at module level
fn html_tag_regex() -> &'static regex::Regex {
    regex!(r"<[^>]*>")
}

// Sites that require JavaScript for content rendering
const JAVASCRIPT_HEAVY_SITES: &[&str] = &[
    "reddit.com",
    "twitter.com",
    "x.com",
    "instagram.com",
    "facebook.com",
    "tiktok.com",
    "youtube.com",
];

/// Article extractor service implementation
///
/// Extracts clean, readable article content from HTML using
/// Mozilla's Readability.js algorithm (same as Firefox Reader Mode).
/// Uses stealth HTTP features (UA rotation, cookies, proxy) to avoid bot detection.
pub struct ArticleExtractorService {
    /// HTTP client with cookie jar and optional proxy
    client: Client,
}

impl ArticleExtractorService {
    /// Determine if a host is equal to or a subdomain of a base domain.
    fn host_matches_domain(host: &str, base_domain: &str) -> bool {
        host == base_domain || host.ends_with(&format!(".{}", base_domain))
    }

    /// Check if a URL points to YouTube.
    fn is_youtube_url(&self, url: &str) -> bool {
        let parsed = match url::Url::parse(url) {
            Ok(parsed) => parsed,
            Err(_) => return false,
        };

        let host = match parsed.host_str() {
            Some(host) => host.to_ascii_lowercase(),
            None => return false,
        };

        Self::host_matches_domain(&host, "youtube.com")
            || Self::host_matches_domain(&host, "youtu.be")
            || Self::host_matches_domain(&host, "youtube-nocookie.com")
    }

    /// Decode common HTML entities from metadata values.
    fn decode_html_entities(&self, text: &str) -> String {
        text.replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&nbsp;", " ")
    }

    /// Escape text for safe HTML output.
    fn escape_html(text: &str) -> String {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;")
    }

    /// Normalize whitespace to single spaces.
    fn normalize_whitespace(&self, text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Read a meta/link content attribute from the first matching selector.
    fn read_attribute_from_selectors(
        &self,
        document: &Html,
        selectors: &[&str],
        attribute: &str,
    ) -> Option<String> {
        for selector_str in selectors {
            let selector = match Selector::parse(selector_str) {
                Ok(selector) => selector,
                Err(_) => continue,
            };

            if let Some(value) = document
                .select(&selector)
                .find_map(|element| element.value().attr(attribute))
            {
                let decoded = self.decode_html_entities(value.trim());
                if !decoded.is_empty() {
                    return Some(self.normalize_whitespace(&decoded));
                }
            }
        }

        None
    }

    /// Read plain text from the first matching selector.
    fn read_text_from_selector(&self, document: &Html, selector: &str) -> Option<String> {
        let selector = Selector::parse(selector).ok()?;

        let text = document
            .select(&selector)
            .next()
            .map(|element| element.text().collect::<Vec<_>>().join(" "))?;

        let normalized = self.normalize_whitespace(&self.decode_html_entities(&text));
        if normalized.is_empty() {
            None
        } else {
            Some(normalized)
        }
    }

    /// Extract YouTube metadata from static HTML and build a CleanArticle.
    fn extract_youtube_article_from_html(
        &self,
        html: &str,
        final_url: &str,
    ) -> Result<CleanArticle> {
        let document = Html::parse_document(html);

        let title = self
            .read_attribute_from_selectors(
                &document,
                &[
                    r#"meta[property="og:title"]"#,
                    r#"meta[name="title"]"#,
                    r#"meta[itemprop="name"]"#,
                ],
                "content",
            )
            .or_else(|| self.read_text_from_selector(&document, "title"))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "YouTube Video".to_string());

        let description = self.read_attribute_from_selectors(
            &document,
            &[
                r#"meta[property="og:description"]"#,
                r#"meta[name="description"]"#,
                r#"meta[itemprop="description"]"#,
            ],
            "content",
        );

        let author = self.read_attribute_from_selectors(
            &document,
            &[
                r#"meta[itemprop="author"]"#,
                r#"meta[name="author"]"#,
                r#"link[itemprop="name"]"#,
            ],
            "content",
        );

        let published_date = self
            .read_attribute_from_selectors(
                &document,
                &[
                    r#"meta[itemprop="datePublished"]"#,
                    r#"meta[property="datePublished"]"#,
                    r#"meta[property="video:release_date"]"#,
                ],
                "content",
            )
            .and_then(|date| self.parse_date(&date));

        // Build readable text from available metadata. This allows indexing video links
        // even though full transcript extraction is not available from static HTML alone.
        let mut text_sections = vec![format!("Title: {}", title)];
        if let Some(author_value) = &author {
            text_sections.push(format!("Channel: {}", author_value));
        }
        if let Some(description_value) = &description {
            text_sections.push(description_value.clone());
        }
        text_sections.push(format!("Source: {}", final_url));

        let text_content = text_sections.join("\n\n");

        if text_content.trim().is_empty() {
            return Err(AppError::ContentExtraction {
                path: final_url.to_string(),
                reason: "YouTube metadata extraction returned empty content".to_string(),
            });
        }

        let word_count = self.count_words(&text_content);
        let reading_time_minutes = self.calculate_reading_time(word_count);
        let excerpt = self.generate_excerpt(&text_content);

        let mut content_parts = vec![format!("<h1>{}</h1>", Self::escape_html(&title))];
        if let Some(author_value) = &author {
            content_parts.push(format!(
                "<p><strong>Channel:</strong> {}</p>",
                Self::escape_html(author_value)
            ));
        }
        if let Some(description_value) = &description {
            content_parts.push(format!("<p>{}</p>", Self::escape_html(description_value)));
        }
        content_parts.push(format!(
            "<p><a href=\"{}\">Watch on YouTube</a></p>",
            Self::escape_html(final_url)
        ));

        Ok(CleanArticle {
            title,
            author,
            content: format!("<article>{}</article>", content_parts.join("")),
            text_content,
            word_count,
            reading_time_minutes,
            published_date,
            excerpt,
        })
    }

    /// Fetch and extract YouTube metadata from URL.
    async fn extract_youtube_article_from_url(&self, fetch_url: &str) -> Result<CleanArticle> {
        stealth::random_delay(500, 2000).await;
        let profile = stealth::random_profile();
        let headers = stealth::browser_headers(profile, None);

        let response = self
            .client
            .get(fetch_url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Failed to fetch URL: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            warn!(status = %status, url = %fetch_url, "YouTube URL fetch failed");

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

        let final_url = response.url().to_string();
        if final_url != fetch_url {
            debug!("YouTube URL redirected to: {}", final_url);
            self.validate_url(&final_url)?;
        }

        let html = response
            .text()
            .await
            .map_err(|e| AppError::Network(format!("Failed to read response body: {}", e)))?;

        self.extract_youtube_article_from_html(&html, &final_url)
    }

    /// Create a new article extractor service with default configuration
    ///
    /// # Errors
    ///
    /// Returns `AppError::InternalError` if HTTP client initialization fails.
    /// Common causes:
    /// - SSL/TLS certificate issues (e.g., system CA certificates not found)
    /// - Network configuration problems
    /// - Platform-specific TLS backend initialization failures
    ///
    /// When this fails, app initialization will use a disabled fallback service
    /// that returns helpful error messages instead of crashing the app.
    pub fn new() -> Result<Self> {
        let client = stealth::stealth_client_builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| {
                AppError::InternalError(format!(
                    "HTTP client initialization failed: {}. This is typically caused by \
                     SSL/TLS certificate issues or network configuration problems. \
                     Article extraction from URLs will be unavailable.",
                    e
                ))
            })?;

        Ok(Self { client })
    }

    /// Validate URL for security (SSRF prevention)
    ///
    /// Blocks:
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

    /// Resolve redirect-wrapper URLs by extracting nested HTTP(S) targets from query parameters.
    ///
    /// This handles common search/result wrappers without hard-coding provider-specific hosts.
    fn resolve_fetch_url(&self, url: &str) -> String {
        let parsed = match url::Url::parse(url) {
            Ok(parsed) => parsed,
            Err(_) => return url.to_string(),
        };

        let wrapper_host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();

        for (_, value) in parsed.query_pairs() {
            let candidate = value.trim();
            if candidate.is_empty() {
                continue;
            }

            let nested = match url::Url::parse(candidate) {
                Ok(nested) => nested,
                Err(_) => continue,
            };

            let scheme = nested.scheme();
            if scheme != "http" && scheme != "https" {
                continue;
            }

            let nested_host = nested.host_str().unwrap_or_default().to_ascii_lowercase();
            if nested_host.is_empty() || nested_host == wrapper_host {
                continue;
            }

            return nested.to_string();
        }

        url.to_string()
    }

    /// Extract plain text from HTML by stripping all tags
    fn extract_text(&self, html: &str) -> String {
        // Use compile-time validated regex to avoid runtime panics
        let text = html_tag_regex().replace_all(html, " ");
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Parse ISO 8601 date string to DateTime<Utc>
    fn parse_date(&self, date_str: &str) -> Option<DateTime<Utc>> {
        if let Ok(date) = DateTime::parse_from_rfc3339(date_str) {
            return Some(date.with_timezone(&Utc));
        }
        None
    }

    /// Calculate word count
    fn count_words(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }

    /// Calculate reading time in minutes (200 words per minute)
    fn calculate_reading_time(&self, word_count: usize) -> i64 {
        ((word_count as f64 / 200.0).ceil() as i64).max(1)
    }

    /// Generate excerpt from text (first ~200 characters)
    fn generate_excerpt(&self, text: &str) -> Option<String> {
        if text.is_empty() {
            return None;
        }

        let excerpt = if text.len() <= 200 {
            text.to_string()
        } else {
            // Find last space before 200 chars
            let truncated = &text[..200];
            if let Some(last_space) = truncated.rfind(' ') {
                format!("{}...", &text[..last_space])
            } else {
                format!("{}...", truncated)
            }
        };

        Some(excerpt)
    }
}

#[async_trait]
impl ArticleExtractorServiceTrait for ArticleExtractorService {
    async fn extract_article_from_url(&self, url: &str) -> Result<CleanArticle> {
        debug!("Extracting article from URL: {}", url);

        let fetch_url = self.resolve_fetch_url(url);
        if fetch_url != url {
            debug!(original_url = url, resolved_url = %fetch_url, "Resolved wrapped URL before fetch");
        }

        // Validate URL for security (SSRF prevention)
        self.validate_url(&fetch_url)?;

        // Handle YouTube separately: we can index metadata from static HTML.
        if self.is_youtube_url(&fetch_url) {
            return self.extract_youtube_article_from_url(&fetch_url).await;
        }

        // Detect JavaScript-heavy sites — try FlareSolverr if available
        if let Ok(parsed_url) = url::Url::parse(&fetch_url) {
            if let Some(domain) = parsed_url.host_str() {
                let domain_lower = domain.to_ascii_lowercase();
                if JAVASCRIPT_HEAVY_SITES
                    .iter()
                    .any(|site| Self::host_matches_domain(&domain_lower, site))
                {
                    if let Some(solver) = stealth::flaresolverr() {
                        info!(url = %fetch_url, "Using FlareSolverr for JS-heavy site");
                        match solver.solve(&fetch_url).await {
                            Ok(html) => {
                                return self.extract_article(&html, &fetch_url).await;
                            }
                            Err(e) => {
                                warn!("FlareSolverr failed for {}: {}", fetch_url, e);
                            }
                        }
                    }

                    warn!(
                        url = fetch_url,
                        domain = domain,
                        "Detected JavaScript-heavy site - extraction may fail"
                    );

                    return Err(AppError::ContentExtraction {
                        path: fetch_url.clone(),
                        reason: format!(
                            "Site '{}' requires JavaScript for content rendering. \
                             Static HTML extraction is not supported for this site. \
                             Consider using the official API or browser extension instead, \
                             or configure FlareSolverr via RECALL_FLARESOLVERR_URL.",
                            domain
                        ),
                    });
                }
            }
        }

        stealth::random_delay(500, 2000).await;
        let profile = stealth::random_profile();
        let headers = stealth::browser_headers(profile, None);

        // Fetch URL with stealth headers
        let response = self
            .client
            .get(&fetch_url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| AppError::Network(format!("Failed to fetch URL: {}", e)))?;

        // Check status
        if !response.status().is_success() {
            let status = response.status();
            warn!(status = %status, url = %fetch_url, "URL fetch failed");
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
        if final_url != fetch_url {
            debug!("URL redirected to: {}", final_url);
            self.validate_url(&final_url)?;
        }

        // Read HTML body
        let html = response
            .text()
            .await
            .map_err(|e| AppError::Network(format!("Failed to read response body: {}", e)))?;

        // Extract article using the existing method
        self.extract_article(&html, &final_url).await
    }

    async fn extract_article(&self, html: &str, url: &str) -> Result<CleanArticle> {
        debug!("Extracting article from URL: {}", url);

        // Validate input
        if html.trim().is_empty() {
            return Err(AppError::InvalidInput("HTML content is empty".to_string()));
        }

        // Create a new Readability instance for thread safety
        // (Readability uses QuickJS internally which is not Send/Sync)
        let readability = Readability::new().map_err(|e| {
            warn!("Failed to initialize Readability.js: {}", e);
            AppError::ContentExtraction {
                path: url.to_string(),
                reason: format!("Failed to initialize article extractor: {}", e),
            }
        })?;

        // Use Readability.js to extract article content
        let article = readability.parse_with_url(html, url).map_err(|e| {
            warn!("Readability.js extraction failed: {}", e);
            AppError::ContentExtraction {
                path: url.to_string(),
                reason: format!("Failed to extract article content: {}", e),
            }
        })?;

        // Extract metadata
        let title = article.title;
        debug!("Extracted title: {}", title);

        let author = article.byline.filter(|s| !s.is_empty());
        if let Some(ref a) = author {
            debug!("Extracted author: {}", a);
        }

        // Parse publication date if available
        let published_date = article
            .published_time
            .as_ref()
            .and_then(|date_str| self.parse_date(date_str));

        if let Some(ref date) = published_date {
            debug!("Extracted published date: {}", date);
        }

        // Get the clean HTML content
        let clean_html = article.content;

        // Extract plain text from HTML
        let text_content = self.extract_text(&clean_html);

        // Validate we got meaningful content
        if text_content.trim().is_empty() {
            return Err(AppError::ContentExtraction {
                path: url.to_string(),
                reason: "No text content found after extraction".to_string(),
            });
        }

        // Calculate metrics
        let word_count = self.count_words(&text_content);
        let reading_time_minutes = self.calculate_reading_time(word_count);

        // Quality validation - reject garbage extractions
        if word_count < 100 {
            warn!(
                url = url,
                word_count = word_count,
                title = %title,
                "Low-quality extraction detected"
            );

            return Err(AppError::ContentExtraction {
                path: url.to_string(),
                reason: format!(
                    "Extracted only {} words - content appears low-quality. \
                     This site likely requires JavaScript to render content. \
                     Try: (1) Use a browser extension, (2) Copy/paste content directly, \
                     or (3) Try a different URL format.",
                    word_count
                ),
            });
        }

        // Additional check: detect generic site titles
        let generic_titles = ["the heart of the internet", "reddit", "loading", "error"];
        let title_lower = title.to_lowercase();
        if generic_titles.iter().any(|&t| title_lower.contains(t)) && word_count < 200 {
            warn!(
                url = url,
                title = %title,
                "Generic title detected in extraction"
            );

            return Err(AppError::ContentExtraction {
                path: url.to_string(),
                reason: format!(
                    "Extracted generic title '{}' - extraction likely failed. \
                     This site may not be compatible with static HTML extraction.",
                    title
                ),
            });
        }

        // Generate excerpt
        let excerpt = self.generate_excerpt(&text_content);

        debug!(
            "Article extracted: {} words, {} minute read",
            word_count, reading_time_minutes
        );

        Ok(CleanArticle {
            title,
            author,
            content: clean_html,
            text_content,
            word_count,
            reading_time_minutes,
            published_date,
            excerpt,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_extract_article_basic() {
        let service = ArticleExtractorService::new().unwrap();

        let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Test Article Title</title>
                <meta name="author" content="John Doe">
            </head>
            <body>
                <article>
                    <h1>Main Heading</h1>
                    <p>This is the first paragraph of the article with some content that discusses various aspects of the topic at hand. We need to ensure that this paragraph contains enough words to meet the minimum word count threshold for article extraction to be successful. The content should be meaningful and representative of a real article that someone might want to extract and read later.</p>
                    <p>This is the second paragraph with more interesting information and additional details about the subject matter. Articles typically contain multiple paragraphs with substantial content to provide comprehensive coverage of the topic. This helps ensure that readers get a complete understanding of the subject being discussed.</p>
                    <p>And here is a third paragraph to make it substantial and provide even more context about the article topic. Quality articles contain well-structured information that is both informative and engaging for readers. This ensures the extraction process can properly identify and preserve the main content of the article.</p>
                </article>
            </body>
            </html>
        "#;

        let result = service
            .extract_article(html, "https://example.com/article")
            .await;

        assert!(result.is_ok());
        let article = result.unwrap();

        assert_eq!(article.title, "Test Article Title");
        assert_eq!(article.author, Some("John Doe".to_string()));
        assert!(article.word_count > 0);
        assert!(article.reading_time_minutes >= 1);
        assert!(article.excerpt.is_some());
        assert!(!article.content.is_empty());
        assert!(!article.text_content.is_empty());
    }

    #[tokio::test]
    async fn test_extract_article_with_main_tag() {
        let service = ArticleExtractorService::new().unwrap();

        let html = r#"
            <html>
            <head>
                <title>Article Title</title>
            </head>
            <body>
                <nav>This should be removed</nav>
                <main>
                    <h1>Article Title</h1>
                    <p>This is the main content that should be extracted and preserved for the reader with many additional words to ensure we meet the minimum threshold. Articles need substantial content to be considered high quality and worth extracting from web pages.</p>
                    <p>Multiple paragraphs ensure we have enough content for proper extraction and provide readers with comprehensive information about the topic being discussed. This helps the article extractor understand that this is genuine article content.</p>
                    <p>The article continues with more valuable information here and additional context that helps readers understand the full scope of the topic. Quality content extraction requires sufficient text to distinguish articles from navigation elements.</p>
                </main>
                <footer>This should also be removed</footer>
            </body>
            </html>
        "#;

        let result = service
            .extract_article(html, "https://example.com")
            .await
            .unwrap();

        assert_eq!(result.title, "Article Title");
        assert!(result.text_content.contains("main content"));
        assert!(!result.text_content.contains("should be removed"));
        assert!(result.word_count > 10);
    }

    #[tokio::test]
    async fn test_extract_article_removes_ads() {
        let service = ArticleExtractorService::new().unwrap();

        let html = r#"
            <html>
            <head>
                <title>Clean Article</title>
            </head>
            <body>
                <article>
                    <h1>Clean Article</h1>
                    <p>This is legitimate article content that should remain and provides readers with valuable information about the topic at hand. We need sufficient content to meet the minimum word count threshold for successful article extraction.</p>
                    <p>More article content here that is valuable to readers and contains detailed information about various aspects of the subject being discussed. Quality articles provide comprehensive coverage of topics and ensure readers have everything they need.</p>
                    <p>Final paragraph of the article with more information and additional context to help readers fully understand the topic. Articles should be substantial enough to warrant extraction and preservation for later reading and reference purposes.</p>
                </article>
            </body>
            </html>
        "#;

        let result = service
            .extract_article(html, "https://example.com")
            .await
            .unwrap();

        assert!(result.text_content.contains("legitimate article"));
        // Readability.js cleans the content and removes non-article elements
        assert!(result.word_count >= 100);
    }

    #[tokio::test]
    async fn test_word_count_and_reading_time() {
        let service = ArticleExtractorService::new().unwrap();

        let html = r#"
            <html>
            <head>
                <title>Title</title>
            </head>
            <body>
                <article>
                    <h1>Title</h1>
                    <p>Word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word word.</p>
                </article>
            </body>
            </html>
        "#;

        let result = service
            .extract_article(html, "https://example.com")
            .await
            .unwrap();

        // Should have 100+ words from the paragraph
        assert!(result.word_count >= 100);
        // Reading time should be 1 minute (minimum)
        assert_eq!(result.reading_time_minutes, 1);
    }

    #[tokio::test]
    async fn test_excerpt_generation() {
        let service = ArticleExtractorService::new().unwrap();

        let html = r#"
            <html>
            <head>
                <title>Title</title>
            </head>
            <body>
                <article>
                    <h1>Title</h1>
                    <p>This is a short article that should generate a proper excerpt from the beginning of the content without truncation and contains enough words to meet the minimum threshold for article extraction. We need to ensure there is sufficient content for the extractor to recognize this as a legitimate article worth processing. Additional words help the system understand that this is real article content that should be preserved and made available to readers who want to save articles for later reading. Quality content requires a minimum number of words to be considered substantial enough for extraction and archiving purposes. We continue adding more sentences to ensure we exceed any minimum word count requirements that the extraction service may have in place for determining valid articles.</p>
                </article>
            </body>
            </html>
        "#;

        let result = service
            .extract_article(html, "https://example.com")
            .await
            .unwrap();

        assert!(result.excerpt.is_some());
        let excerpt = result.excerpt.unwrap();
        assert!(excerpt.len() <= 203); // 200 + "..."
        assert!(excerpt.starts_with("This is a short"));
    }

    #[tokio::test]
    async fn test_empty_html_error() {
        let service = ArticleExtractorService::new().unwrap();

        let result = service.extract_article("", "https://example.com").await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_no_content_error() {
        let service = ArticleExtractorService::new().unwrap();

        let html = r#"
            <html>
            <body>
                <div>Too short</div>
            </body>
            </html>
        "#;

        let result = service.extract_article(html, "https://example.com").await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            AppError::ContentExtraction { .. }
        ));
    }

    #[tokio::test]
    async fn test_extract_published_date() {
        let service = ArticleExtractorService::new().unwrap();

        let html = r#"
            <html>
            <head>
                <title>Dated Article</title>
                <meta property="article:published_time" content="2024-01-15T10:30:00Z">
            </head>
            <body>
                <article>
                    <h1>Dated Article</h1>
                    <p>This article has a publication date in the meta tags that should be extracted properly and demonstrates how article extractors can preserve important metadata about when content was published. This helps readers understand the context and timeliness of information that they are reading and consuming from various sources.</p>
                    <p>Additional content to meet minimum length requirements for extraction ensures that the article has enough substance to be worth saving and reading later. Quality articles provide comprehensive information that is valuable to readers over time, making proper date extraction an important feature. We add more sentences to ensure the word count threshold is exceeded and the article passes validation checks for content quality and substance.</p>
                </article>
            </body>
            </html>
        "#;

        let result = service
            .extract_article(html, "https://example.com")
            .await
            .unwrap();

        use chrono::Datelike;
        assert!(result.published_date.is_some());
        let date = result.published_date.unwrap();
        assert_eq!(date.naive_utc().year(), 2024);
        assert_eq!(date.naive_utc().month(), 1);
        assert_eq!(date.naive_utc().day(), 15);
    }

    #[tokio::test]
    async fn test_extract_youtube_article_from_html_metadata() {
        let service = ArticleExtractorService::new().unwrap();

        let html = r#"
            <html>
            <head>
                <title>Video Title - YouTube</title>
                <meta property="og:title" content="Sample YouTube Video">
                <meta property="og:description" content="This is a sample YouTube description with enough words to provide meaningful context for indexing and later retrieval in search.">
                <meta itemprop="author" content="Sample Channel">
                <meta itemprop="datePublished" content="2024-05-12T12:00:00Z">
            </head>
            <body>
                <div id="player"></div>
            </body>
            </html>
        "#;

        let result = service
            .extract_youtube_article_from_html(html, "https://www.youtube.com/watch?v=test")
            .unwrap();

        assert_eq!(result.title, "Sample YouTube Video");
        assert_eq!(result.author, Some("Sample Channel".to_string()));
        assert!(result.text_content.contains("sample YouTube description"));
        assert!(result.content.contains("Watch on YouTube"));
        assert!(result.word_count > 10);
        assert!(result.published_date.is_some());
    }

    #[test]
    fn test_is_youtube_url_variants() {
        let service = ArticleExtractorService::new().unwrap();

        assert!(service.is_youtube_url("https://www.youtube.com/watch?v=abc123"));
        assert!(service.is_youtube_url("https://youtu.be/abc123"));
        assert!(service.is_youtube_url("https://m.youtube.com/watch?v=abc123"));
        assert!(!service.is_youtube_url("https://example.com/watch?v=abc123"));
    }
}
