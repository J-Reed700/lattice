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

use crate::features::function_calling::dto::{CleanArticle, UrlPreview};
use crate::features::web::dto::{GetUrlPreviewRequestDto, IngestWebUrlRequestDto};
use crate::infrastructure::audit::{get_audit_logger, AuditAction, AuditEvent, AuditResult};
use crate::interfaces::di::Container;
use crate::shared::error::AppError;
// TODO: Fix web_ingestion module test errors before enabling
// use crate::features::web::ingestion::types::{is_safe_url, normalize_url};
use serde::{Deserialize, Serialize};
use tauri::State;

/// Response from web ingestion command
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WebIngestResponse {
    /// Unique document ID
    pub document_id: String,
    /// Final URL (after redirects)
    pub url: String,
    /// Document title
    pub title: String,
    /// Word count
    pub word_count: usize,
    /// Number of chunks created
    pub chunks: usize,
    /// Site name (if available)
    pub site_name: Option<String>,
    /// Author (if available)
    pub author: Option<String>,
    /// Estimated reading time in minutes
    pub reading_time_minutes: Option<i64>,
}

fn normalize_optional_id(value: Option<String>) -> Option<String> {
    value
        .map(|raw| raw.trim().to_string())
        .filter(|trimmed| !trimmed.is_empty())
}

async fn ensure_space_exists(container: &Container, space_id: &str) -> Result<(), AppError> {
    let exists = container
        .document_scope()
        .space_exists(space_id)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to verify target space '{}' for web import: {}",
                space_id, e
            ))
        })?;
    if !exists {
        return Err(AppError::InvalidInput(format!(
            "Space not found for web import: {}",
            space_id
        )));
    }
    Ok(())
}

async fn ensure_conversation_exists(
    container: &Container,
    conversation_id: &str,
) -> Result<(), AppError> {
    let exists = container
        .document_scope()
        .conversation_exists(conversation_id)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to verify target conversation '{}' for web import: {}",
                conversation_id, e
            ))
        })?;
    if !exists {
        return Err(AppError::InvalidInput(format!(
            "Conversation not found for web import: {}",
            conversation_id
        )));
    }
    Ok(())
}

async fn assign_document_to_space(
    container: &Container,
    document_id: &str,
    space_id: &str,
) -> Result<(), AppError> {
    container
        .document_scope()
        .assign_documents(&[document_id.to_string()], space_id)
        .await
        .map_err(|e| {
            AppError::Database(format!(
                "Failed to assign imported web document '{}' to space '{}': {}",
                document_id, space_id, e
            ))
        })
}

/// Ingest web content with full extraction and indexing pipeline
///
/// Fetches web page, extracts main content using Mozilla Readability, chunks the text,
/// generates embeddings, and indexes for semantic search. Returns document metadata
/// including word count, chunk count, and reading time estimate.
///
/// # Arguments
///
/// * `url` - Web URL to ingest (http/https only)
/// * `container` - Service container with dependencies
///
/// # Returns
///
/// * `Ok(WebIngestResponse)` - Ingested document details with metadata
/// * `Err(AppError::RateLimitExceeded)` - Too many ingest requests
/// * `Err(AppError)` - Ingestion failed (network, extraction, indexing errors)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface WebIngestResponse {
///   documentId: string;
///   url: string;
///   title: string;
///   wordCount: number;
///   chunks: number;
///   siteName?: string;
///   author?: string;
///   readingTimeMinutes?: number;
/// }
///
/// // Ingest web article
/// const result = await invoke<WebIngestResponse>('ingest_web_url', {
///   url: 'https://example.com/article'
/// });
///
/// console.log(`Ingested: ${result.title}`);
/// console.log(`Created ${result.chunks} chunks from ${result.wordCount} words`);
/// console.log(`Reading time: ${result.readingTimeMinutes} minutes`);
///
/// // Show ingestion progress
/// const ingestWithProgress = async (url: string) => {
///   showProgress('Fetching URL...');
///   const result = await invoke<WebIngestResponse>('ingest_web_url', { url });
///
///   showSuccess(`Ingested: ${result.title}`);
///   return result.documentId;
/// };
///
/// // Batch ingest URLs
/// const ingestMultipleUrls = async (urls: string[]) => {
///   const results = [];
///   for (const url of urls) {
///     try {
///       const result = await invoke<WebIngestResponse>('ingest_web_url', { url });
///       results.push(result);
///     } catch (error) {
///       console.error(`Failed to ingest ${url}:`, error);
///     }
///   }
///   return results;
/// };
/// ```
///
/// # Pipeline Steps
///
/// 1. **Rate Limiting**: Check web ingest rate limit
/// 2. **URL Validation**: Verify URL safety (SSRF prevention)
/// 3. **Fetch**: Download HTML content
/// 4. **Extract**: Use Mozilla Readability to extract main content
/// 5. **Chunk**: Split content into semantic chunks
/// 6. **Embed**: Generate embeddings for each chunk
/// 7. **Index**: Store in vector database
/// 8. **Audit**: Log successful ingestion
///
/// # Security
///
/// **Rate Limiting (CWE-770)**: Max web ingest requests per minute
/// **Audit Logging (CWE-778)**: All ingestions logged with URL and document ID
/// **SSRF Prevention**: URL validation in service layer
///
/// # Performance
///
/// - **Ingest Time**: ~2-10 seconds (depends on page size, network)
/// - **Chunking**: Semantic chunking with overlap
/// - **Embedding**: Batched for efficiency
///
/// # Use Cases
///
/// - **Research**: Index articles for later semantic search
/// - **Knowledge Base**: Build indexed corpus from web sources
/// - **Reading List**: Import articles for offline reading
///
/// # Architecture
///
/// Thin controller delegating to `IngestWebUrlUseCase` with rate limiting + audit logging
#[tracing::instrument(skip(container), fields(url = %url))]
pub async fn ingest_web_url(
    url: String,
    space_id: Option<String>,
    conversation_id: Option<String>,
    container: State<'_, Container>,
) -> Result<WebIngestResponse, AppError> {
    let requested_space_id = normalize_optional_id(space_id);
    let requested_conversation_id = normalize_optional_id(conversation_id);

    if let Some(space_id) = requested_space_id.as_deref() {
        ensure_space_exists(&container, space_id).await?;
    }

    if let Some(conversation_id) = requested_conversation_id.as_deref() {
        ensure_conversation_exists(&container, conversation_id).await?;
    }

    // Ensure embedding model is available before starting ingestion.
    // This pre-loads the active embedding model into cache if configured.
    match container.get_or_load_embedding().await {
        Ok(embedding) => match embedding.is_ready().await {
            Ok(true) => {
                tracing::info!("Embedding model ready for web ingestion");
            }
            Ok(false) => {
                return Err(AppError::AiModelsNotInstalled(
                        "AI models are not installed. This feature requires embedding models. \
                         Please download models from Settings → Models to enable this functionality."
                            .to_string(),
                    ));
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to verify embedding readiness before web ingest: {}",
                    e
                );
            }
        },
        Err(e) => return Err(e),
    }

    // 1. Rate limiting
    if let Err(e) = container
        .security_context()
        .rate_limiters()
        .web_ingest
        .check_rate_limit("web_ingest")
        .await
    {
        let audit_logger = get_audit_logger();
        let event = AuditEvent::new(
            AuditAction::WebContentIngested,
            AuditResult::failure(e.to_string()),
        )
        .with_resource_id(&url)
        .with_metadata("operation", "ingest_web_url")
        .with_metadata("error", e.to_string());
        if let Err(log_err) = audit_logger.log(event).await {
            tracing::warn!("Failed to write audit log: {}", log_err);
        }
        return Err(AppError::RateLimitExceeded(e.to_string()));
    }

    // 2. Get use case
    let ingest_use_case = container.ingest_web_url_use_case();

    // 3. Ingest URL via use case
    let result = match ingest_use_case
        .execute(IngestWebUrlRequestDto { url: url.clone() })
        .await
    {
        Ok(res) => res,
        Err(e) => {
            tracing::error!("Web ingestion failed for URL {}: {:?}", url, e);
            container.metrics().record_indexing_error();
            let audit_logger = get_audit_logger();
            let event = AuditEvent::new(
                AuditAction::WebContentIngested,
                AuditResult::failure(e.to_string()),
            )
            .with_resource_id(&url)
            .with_metadata("operation", "ingest_web_url")
            .with_metadata("error", e.to_string());
            if let Err(log_err) = audit_logger.log(event).await {
                tracing::warn!("Failed to write audit log: {}", log_err);
            }
            return Err(e);
        }
    };

    // 4. Scope imported document into an explicit space when provided.
    if let Some(space_id) = requested_space_id.as_deref() {
        assign_document_to_space(&container, &result.document_id, space_id).await?;
    }

    // 5. Link imported document to conversation context when provided.
    if let Some(conversation_id) = requested_conversation_id.as_deref() {
        container
            .conversation_service()
            .add_document_reference(conversation_id, result.document_id.clone(), None, None)
            .await
            .map_err(|e| {
                AppError::Database(format!(
                    "Imported web document '{}' but failed to link conversation '{}': {}",
                    result.document_id, conversation_id, e
                ))
            })?;
    }

    // 6. Record metrics after success
    container.metrics().record_indexing_operation();

    // 7. Audit success
    let audit_logger = get_audit_logger();
    let event = AuditEvent::new(AuditAction::WebContentIngested, AuditResult::success())
        .with_resource_id(&url)
        .with_metadata("operation", "ingest_web_url")
        .with_metadata("document_id", &result.document_id)
        .with_metadata("title", &result.title)
        .with_metadata("chunks", result.chunks_created.to_string());

    if let Err(e) = audit_logger.log(event).await {
        tracing::warn!("Failed to write audit log: {}", e);
    }

    // 8. Return response
    Ok(WebIngestResponse {
        document_id: result.document_id,
        url: result.url,
        title: result.title,
        word_count: result.word_count,
        chunks: result.chunks_created,
        site_name: result.site_name,
        author: result.author,
        reading_time_minutes: result.reading_time_minutes,
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
