//! Web Content Ingestion Commands
//!
//! Thin command controllers for ingesting, previewing, and extracting web content following
//! DDD pattern. Provides three distinct operations for web content handling: full ingestion
//! with indexing, metadata preview, and article extraction.
//!
//! # Commands (3 total)
//!
//! - `ingest_web_url` - Fetch, extract, and index web content
//! - `fetch_url_preview` - Get URL metadata without indexing
//! - `extract_article` - Extract article content with Mozilla Readability
//!
//! # Security Features
//!
//! - **Rate Limiting (CWE-770)**: Prevents DoS via excessive web requests
//! - **Audit Logging (CWE-778)**: All web access operations logged
//! - **SSRF Prevention**: URL validation in service layer
//!
//! # Architecture
//!
//! Commands delegate to specialized services:
//! - `WebIngestionService` - Full ingestion pipeline
//! - `WebCaptureService` - Metadata extraction (OpenGraph, Schema.org)
//! - `ArticleExtractorService` - Clean article extraction (Readability)
//!
//! # Use Cases
//!
//! - **Research**: Index web articles for semantic search
//! - **Link Preview**: Display rich metadata for URLs
//! - **Reading Mode**: Extract clean article content for display
//! - **Batch Import**: Bulk ingestion of web resources

use crate::application::contracts::web::{CleanArticle, UrlPreview};
use crate::features::web::dto::GetUrlPreviewRequestDto;
use crate::infrastructure::audit::{get_audit_logger, AuditAction, AuditEvent, AuditResult};
use crate::interfaces::di::Container;
use crate::shared::error::AppError;
// TODO: Fix web_ingestion module test errors before enabling
// use crate::features::web::ingestion::types::{is_safe_url, normalize_url};
use serde::{Deserialize, Serialize};
use tauri::State;

mod ingest;

pub use ingest::{ingest_web_url, WebIngestResponse};

/// A web page as the reader shows it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WebPageDto {
    /// The URL after redirects.
    pub url: String,
    /// Page title, when the page gave one.
    pub title: Option<String>,
    /// Extracted article text — the same text the model was given.
    pub text: String,
    pub word_count: u32,
    /// RFC 3339. When the text was actually fetched, not when it was served.
    pub fetched_at: String,
    /// True when this came out of the page cache rather than off the network.
    pub from_cache: bool,
}

/// Read a web page for the reader pane
///
/// Goes through the same fetch path the model's `fetch_url_content` tool uses,
/// which means a page the turn already read is answered from the page cache
/// without touching the network — the point of the reader is to show the text
/// the model actually read, and asking the site again could return something
/// else. A page nobody has read yet costs one validated fetch.
///
/// Deliberately not rate limited, unlike the ingest and preview commands: this
/// is one page per click, most of them free, and the shared web-ingest limiter
/// allows five a minute — a user reading through a turn's citations would hit
/// it in seconds. The fetch underneath is still SSRF-validated and paced per
/// site.
///
/// # Arguments
///
/// * `url` - Web URL to read (http/https only)
/// * `container` - Service container with dependencies
///
/// # Returns
///
/// * `Ok(WebPageDto)` - The page text, with when it was read and whether it
///   came from the cache
/// * `Err(AppError)` - Invalid or blocked URL, or the fetch failed
#[tracing::instrument(skip(container), fields(url = %url))]
pub async fn read_web_page(
    url: String,
    container: State<'_, Container>,
) -> Result<WebPageDto, AppError> {
    let read = match container.web_service().read_page(&url).await {
        Ok(read) => read,
        Err(e) => {
            let audit_logger = get_audit_logger();
            let event = AuditEvent::new(
                AuditAction::WebContentAccessed,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&url)
            .with_metadata("operation", "read_web_page")
            .with_metadata("error", e.to_string());
            if let Err(log_err) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", log_err);
            }
            return Err(e);
        }
    };

    let audit_logger = get_audit_logger();
    let event = AuditEvent::new(AuditAction::WebContentAccessed, AuditResult::success())
        .with_resource_id(&url)
        .with_metadata("operation", "read_web_page")
        .with_metadata("from_cache", read.output.from_cache.to_string())
        .with_metadata("word_count", read.output.word_count.to_string());
    if let Err(e) = audit_logger.log(event).await {
        tracing::warn!("Failed to write audit log: {}", e);
    }

    Ok(WebPageDto {
        url: read.output.url,
        title: read.output.title,
        text: read.output.content,
        word_count: u32::try_from(read.output.word_count).unwrap_or(u32::MAX),
        fetched_at: read.fetched_at.to_rfc3339(),
        from_cache: read.output.from_cache,
    })
}

/// Fetch URL metadata preview without indexing content
///
/// Extracts rich metadata from URL including OpenGraph tags, Schema.org data, and HTML meta
/// tags. Provides title, description, images, author, and site information for link preview
/// display. Does not index content - lightweight metadata-only operation.
///
/// # Arguments
///
/// * `url` - Web URL to preview
/// * `container` - Service container with dependencies
///
/// # Returns
///
/// * `Ok(UrlPreview)` - Rich metadata including title, description, images
/// * `Err(AppError::RateLimitExceeded)` - Too many preview requests
/// * `Err(AppError)` - Preview failed (network, parsing errors)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface UrlPreview {
///   url: string;
///   title: string;
///   description?: string;
///   image?: string;
///   siteName?: string;
///   author?: string;
///   publishedTime?: string;
///   wordCount: number;
/// }
///
/// // Fetch URL preview
/// const preview = await invoke<UrlPreview>('fetch_url_preview', {
///   url: 'https://example.com/article'
/// });
///
/// console.log(`Title: ${preview.title}`);
/// console.log(`Description: ${preview.description}`);
///
/// // Display link preview card
/// const displayLinkPreview = async (url: string) => {
///   const preview = await invoke<UrlPreview>('fetch_url_preview', { url });
///
///   return (
///     <div className="link-preview">
///       {preview.image && <img src={preview.image} alt={preview.title} />}
///       <h3>{preview.title}</h3>
///       <p>{preview.description}</p>
///       <span>{preview.siteName}</span>
///     </div>
///   );
/// };
///
/// // Paste URL handler with preview
/// const handleUrlPaste = async (url: string) => {
///   const preview = await invoke<UrlPreview>('fetch_url_preview', { url });
///
///   showPreviewDialog({
///     title: preview.title,
///     description: preview.description,
///     actions: [
///       { label: 'Ingest', onClick: () => ingest_web_url(url) },
///       { label: 'Cancel', onClick: () => close() }
///     ]
///   });
/// };
/// ```
///
/// # Metadata Sources
///
/// Extracts metadata from (in priority order):
/// 1. **OpenGraph**: `og:title`, `og:description`, `og:image`, `og:site_name`
/// 2. **Twitter Cards**: `twitter:title`, `twitter:description`, `twitter:image`
/// 3. **Schema.org**: JSON-LD structured data
/// 4. **HTML Meta**: `<meta name="description">`, `<title>` tag
///
/// # Security
///
/// **Rate Limiting (CWE-770)**: Web capture operations rate limited
/// **Audit Logging (CWE-778)**: All preview operations logged
/// **SSRF Prevention**: URL validation in service layer
///
/// # Performance
///
/// - **Preview Time**: ~500ms-2s (depends on page size, network)
/// - **Lightweight**: Does not extract full content or index
/// - **Fast Metadata**: Optimized HTML parsing
///
/// # Use Cases
///
/// - **Link Preview**: Display rich previews before ingesting
/// - **URL Validation**: Verify URL content before batch import
/// - **Content Discovery**: Browse metadata without full ingestion
///
/// # Architecture
///
/// Thin controller delegating to `GetUrlPreviewUseCase` with rate limiting + audit logging
#[tracing::instrument(skip(container), fields(url = %url))]
pub async fn fetch_url_preview(
    url: String,
    container: State<'_, Container>,
) -> Result<UrlPreview, AppError> {
    // Rate limiting
    if let Err(e) = container
        .security_context()
        .rate_limiters()
        .web_ingest
        .check_rate_limit("web_capture")
        .await
    {
        let audit_logger = get_audit_logger();
        let event = AuditEvent::new(
            AuditAction::WebContentAccessed,
            AuditResult::failure(e.to_string()),
        )
        .with_resource_id(&url)
        .with_metadata("operation", "fetch_url_preview")
        .with_metadata("error", e.to_string());
        if let Err(log_err) = audit_logger.log(event).await {
            tracing::warn!("Failed to write audit log: {}", log_err);
        }
        return Err(AppError::RateLimitExceeded(e.to_string()));
    }

    let preview_use_case = container.get_url_preview_use_case();

    let preview = match preview_use_case
        .execute(GetUrlPreviewRequestDto { url: url.clone() })
        .await
    {
        Ok(preview) => preview,
        Err(e) => {
            let audit_logger = get_audit_logger();
            let event = AuditEvent::new(
                AuditAction::WebContentAccessed,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&url)
            .with_metadata("operation", "fetch_url_preview")
            .with_metadata("error", e.to_string());
            if let Err(log_err) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", log_err);
            }
            return Err(e);
        }
    };

    let preview = UrlPreview {
        url: preview.url,
        title: preview.title,
        description: preview.description,
        site_name: preview.site_name,
        image: preview.image,
        author: None,
        published_date: None,
        word_count: 0,
        reading_time_minutes: 0,
        language: None,
        content_type: None,
        keywords: Vec::new(),
    };

    // Audit success
    let audit_logger = get_audit_logger();
    let event = AuditEvent::new(AuditAction::WebContentAccessed, AuditResult::success())
        .with_resource_id(&url)
        .with_metadata("operation", "fetch_url_preview")
        .with_metadata("title", &preview.title)
        .with_metadata("word_count", preview.word_count.to_string());

    if let Err(e) = audit_logger.log(event).await {
        tracing::warn!("Failed to write audit log: {}", e);
    }

    Ok(preview)
}

/// Extract clean article content using Mozilla Readability algorithm
///
/// Extracts main article content from web page, removing ads, navigation, sidebars, and other
/// clutter. Uses Mozilla's Readability algorithm to isolate article text. Returns clean content
/// without indexing - useful for reading mode display or content preview.
///
/// # Arguments
///
/// * `url` - Web URL to extract article from
/// * `container` - Service container with dependencies
///
/// # Returns
///
/// * `Ok(CleanArticle)` - Extracted article with clean content and metadata
/// * `Err(AppError::RateLimitExceeded)` - Too many extraction requests
/// * `Err(AppError)` - Extraction failed (network, parsing, not an article)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface CleanArticle {
///   title: string;
///   content: string;          // Clean HTML content
///   textContent: string;      // Plain text content
///   excerpt?: string;         // Article summary/excerpt
///   byline?: string;          // Author byline
///   siteName?: string;        // Site name
///   wordCount: number;
///   publishedTime?: string;
/// }
///
/// // Extract article for reading mode
/// const article = await invoke<CleanArticle>('extract_article', {
///   url: 'https://example.com/article'
/// });
///
/// console.log(`Article: ${article.title}`);
/// console.log(`Words: ${article.wordCount}`);
///
/// // Display in reading mode
/// const displayReadingMode = async (url: string) => {
///   const article = await invoke<CleanArticle>('extract_article', { url });
///
///   return (
///     <article className="reading-mode">
///       <h1>{article.title}</h1>
///       {article.byline && <p className="byline">{article.byline}</p>}
///       <div dangerouslySetInnerHTML={{ __html: article.content }} />
///     </article>
///   );
///};
///
/// // Extract before ingesting (preview content)
/// const previewBeforeIngest = async (url: string) => {
///   const article = await invoke<CleanArticle>('extract_article', { url });
///
///   const shouldIngest = await confirmDialog({
///     title: article.title,
///     message: `Ingest ${article.wordCount} words?`,
///     content: article.excerpt
///   });
///
///   if (shouldIngest) {
///     await invoke('ingest_web_url', { url });
///   }
/// };
///
/// // Convert article to plain text
/// const getPlainText = async (url: string) => {
///   const article = await invoke<CleanArticle>('extract_article', { url });
///   return article.textContent;
/// };
/// ```
///
/// # Extraction Algorithm
///
/// Uses **Mozilla Readability**:
/// 1. Parse HTML with DOM tree
/// 2. Score elements by content likelihood
/// 3. Remove low-score elements (ads, nav, footer)
/// 4. Extract main content container
/// 5. Clean up formatting
/// 6. Return clean HTML + plain text
///
/// **Note**: Works best with article-style content (blogs, news). May fail on non-article
/// pages (landing pages, product pages, social media).
///
/// # Security
///
/// **Rate Limiting (CWE-770)**: Web ingest operations rate limited
/// **Audit Logging (CWE-778)**: All extraction operations logged
/// **SSRF Prevention**: URL validation in service layer
///
/// # Performance
///
/// - **Extraction Time**: ~1-3 seconds (depends on page complexity)
/// - **DOM Parsing**: Full HTML parsing required
/// - **Algorithm**: Readability scoring is computationally intensive
///
/// # Use Cases
///
/// - **Reading Mode**: Display clean article without clutter
/// - **Content Preview**: Preview article before ingesting
/// - **Text Extraction**: Get plain text for processing
/// - **Offline Reading**: Extract content for offline storage
///
/// # Limitations
///
/// - **Article Detection**: May fail on non-article pages
/// - **Dynamic Content**: JavaScript-rendered content not supported
/// - **Paywalls**: Cannot extract behind authentication/paywalls
///
/// # Architecture
///
/// Thin controller delegating to `ArticleExtractorService` with rate limiting + audit logging
#[tracing::instrument(skip(container), fields(url = %url))]
pub async fn extract_article(
    url: String,
    container: State<'_, Container>,
) -> Result<CleanArticle, AppError> {
    // Rate limiting
    if let Err(e) = container
        .security_context()
        .rate_limiters()
        .web_ingest
        .check_rate_limit("extract_article")
        .await
    {
        let audit_logger = get_audit_logger();
        let event = AuditEvent::new(
            AuditAction::WebContentAccessed,
            AuditResult::failure(e.to_string()),
        )
        .with_resource_id(&url)
        .with_metadata("operation", "extract_article")
        .with_metadata("error", e.to_string());
        if let Err(log_err) = audit_logger.log(event).await {
            tracing::warn!("Failed to write audit log: {}", log_err);
        }
        return Err(AppError::RateLimitExceeded(e.to_string()));
    }

    let article_extractor = container.article_extractor_service();

    let article = match article_extractor.extract_article_from_url(&url).await {
        Ok(article) => article,
        Err(e) => {
            let audit_logger = get_audit_logger();
            let event = AuditEvent::new(
                AuditAction::WebContentAccessed,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&url)
            .with_metadata("operation", "extract_article")
            .with_metadata("error", e.to_string());
            if let Err(log_err) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", log_err);
            }
            return Err(e);
        }
    };

    // Audit success
    let audit_logger = get_audit_logger();
    let event = AuditEvent::new(AuditAction::WebContentAccessed, AuditResult::success())
        .with_resource_id(&url)
        .with_metadata("operation", "extract_article")
        .with_metadata("title", &article.title)
        .with_metadata("word_count", article.word_count.to_string());

    if let Err(e) = audit_logger.log(event).await {
        tracing::warn!("Failed to write audit log: {}", e);
    }

    Ok(article)
}

// #[cfg(test)]
// mod tests { ... }
