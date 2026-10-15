//! Ingesting a web page, and filing it under a space or conversation.

use crate::features::web::dto::IngestWebUrlRequestDto;
use crate::interfaces::di::Container;
use lattice_core::shared::error::AppError;
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
///
/// # Security
///
/// **Rate Limiting (CWE-770)**: Max web ingest requests per minute
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
/// Thin controller delegating to `IngestWebUrlUseCase` with rate limiting
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
