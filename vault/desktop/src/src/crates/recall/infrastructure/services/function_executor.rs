//! Function executor service
//!
//! Routes and executes function calls from LLMs to appropriate handlers.
//!
//! # Architecture
//!
//! - **Router**: Routes function calls by name to handlers
//! - **Security**: Rate limiting, input validation, audit logging
//! - **Error Handling**: Comprehensive error handling with user-friendly messages
//!
//! # Example
//! ```rust,no_run
//! use vault_desktop::infrastructure::services::function_executor::FunctionExecutor;
//! use vault_desktop::domain::function_call::FunctionCall;
//! use serde_json::json;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let executor = FunctionExecutor::new(/* dependencies */);
//!
//! let call = FunctionCall::new(
//!     "call_123",
//!     "semantic_search",
//!     json!({"query": "machine learning", "limit": 5})
//! );
//!
//! let result = executor.execute(call).await?;
//! assert!(result.success);
//! # Ok(())
//! # }
//! ```

use crate::application::dtos::function_calling_dto::*;
use crate::application::dtos::settings::CustomToolSettingsDto;
use crate::application::ports::{
    ChunkRepositoryPort, DocumentRepository, FavoritesRepositoryPort, FileStoragePort,
    RecentDocumentsRepositoryPort,
};
use crate::domain::function_call::{FunctionCall, FunctionResult};
use crate::infrastructure::search::hybrid::{HybridSearchResult, SearchMode as HybridSearchMode};
use crate::infrastructure::search::service::SearchResult as InfraSearchResult;
use crate::infrastructure::services::traits::{
    BM25SearchTrait, EmbeddingServiceTrait, FunctionExecutorTrait, FunctionRegistryTrait,
    HybridSearchTrait, SearchServiceTrait, TagServiceTrait, WebServiceTrait,
};
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use jsonschema::JSONSchema;
use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE, USER_AGENT};
use reqwest::Client;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Instant;
use tracing::{debug, error, info};
use url::Url;

const TOOL_SEMANTIC_SEARCH: &str = "semantic_search";
const TOOL_GET_DOCUMENT: &str = "get_document";
const TOOL_LIST_DOCUMENTS: &str = "list_documents";
const TOOL_WEB_SEARCH: &str = "web_search";
const TOOL_FETCH_URL_CONTENT: &str = "fetch_url_content";
const TOOL_WIKI_SEARCH: &str = "wiki_search";
const TOOL_WIKI_SUMMARY: &str = "wiki_summary";
const WIKIPEDIA_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

/// Function executor implementation
///
/// Routes function calls to appropriate handlers with security controls.
pub struct FunctionExecutor {
    /// Function registry for validation
    registry: Arc<dyn FunctionRegistryTrait>,

    /// Embedding service for semantic search
    embedding_service: Arc<dyn EmbeddingServiceTrait>,

    /// Search service for semantic/vector search
    search_service: Arc<dyn SearchServiceTrait>,

    /// BM25 search service for keyword search
    bm25_service: Arc<dyn BM25SearchTrait>,

    /// Hybrid search service
    hybrid_service: Arc<dyn HybridSearchTrait>,

    /// Document repository for document operations (DDD ports)
    document_repository: Arc<dyn DocumentRepository>,

    /// Chunk repository for chunk counts
    chunk_repository: Arc<dyn ChunkRepositoryPort>,

    /// Tag service for document tags
    tag_service: Arc<dyn TagServiceTrait>,

    /// Favorites repository for favorite status
    favorites_repository: Arc<dyn FavoritesRepositoryPort>,

    /// Recent documents repository for recent filters
    recent_documents_repository: Arc<dyn RecentDocumentsRepositoryPort>,

    /// File storage for document content and metadata
    file_storage: Arc<dyn FileStoragePort>,

    /// Web service for web search and URL fetching (Phase 2)
    web_service: Arc<dyn WebServiceTrait>,

    /// Custom tools loaded from user settings.
    custom_tools: RwLock<HashMap<String, CustomToolSettingsDto>>,

    /// Shared HTTP client for keyless API integrations.
    http_client: Client,
}

impl FunctionExecutor {
    /// Create a new function executor
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        registry: Arc<dyn FunctionRegistryTrait>,
        embedding_service: Arc<dyn EmbeddingServiceTrait>,
        search_service: Arc<dyn SearchServiceTrait>,
        bm25_service: Arc<dyn BM25SearchTrait>,
        hybrid_service: Arc<dyn HybridSearchTrait>,
        document_repository: Arc<dyn DocumentRepository>,
        chunk_repository: Arc<dyn ChunkRepositoryPort>,
        tag_service: Arc<dyn TagServiceTrait>,
        favorites_repository: Arc<dyn FavoritesRepositoryPort>,
        recent_documents_repository: Arc<dyn RecentDocumentsRepositoryPort>,
        file_storage: Arc<dyn FileStoragePort>,
        web_service: Arc<dyn WebServiceTrait>,
    ) -> Self {
        Self::new_with_custom_tools(
            registry,
            embedding_service,
            search_service,
            bm25_service,
            hybrid_service,
            document_repository,
            chunk_repository,
            tag_service,
            favorites_repository,
            recent_documents_repository,
            file_storage,
            web_service,
            HashMap::new(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_custom_tools(
        registry: Arc<dyn FunctionRegistryTrait>,
        embedding_service: Arc<dyn EmbeddingServiceTrait>,
        search_service: Arc<dyn SearchServiceTrait>,
        bm25_service: Arc<dyn BM25SearchTrait>,
        hybrid_service: Arc<dyn HybridSearchTrait>,
        document_repository: Arc<dyn DocumentRepository>,
        chunk_repository: Arc<dyn ChunkRepositoryPort>,
        tag_service: Arc<dyn TagServiceTrait>,
        favorites_repository: Arc<dyn FavoritesRepositoryPort>,
        recent_documents_repository: Arc<dyn RecentDocumentsRepositoryPort>,
        file_storage: Arc<dyn FileStoragePort>,
        web_service: Arc<dyn WebServiceTrait>,
        custom_tools: HashMap<String, CustomToolSettingsDto>,
    ) -> Self {
        Self {
            registry,
            embedding_service,
            search_service,
            bm25_service,
            hybrid_service,
            document_repository,
            chunk_repository,
            tag_service,
            favorites_repository,
            recent_documents_repository,
            file_storage,
            web_service,
            custom_tools: RwLock::new(custom_tools),
            http_client: Client::new(),
        }
    }

    fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
        if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
            return Some(parsed.with_timezone(&Utc));
        }

        NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
            .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S"))
            .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f"))
            .ok()
            .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
    }

    /// Normalize tool date filters to RFC3339 to satisfy JSON schema validation.
    ///
    /// LLMs sometimes emit date-only values (e.g. `2000-01-01`) or naive datetime
    /// strings. We canonicalize those into UTC RFC3339 before validation/deserialization.
    fn normalize_tool_arguments(
        function_name: &str,
        arguments: &serde_json::Value,
    ) -> serde_json::Value {
        if !matches!(function_name, TOOL_SEMANTIC_SEARCH | TOOL_LIST_DOCUMENTS) {
            return arguments.clone();
        }

        let mut normalized = arguments.clone();
        let Some(object) = normalized.as_object_mut() else {
            return normalized;
        };

        Self::normalize_datetime_filter(object, "date_from", false);
        Self::normalize_datetime_filter(object, "date_to", true);

        normalized
    }

    fn normalize_datetime_filter(
        object: &mut serde_json::Map<String, serde_json::Value>,
        key: &str,
        end_of_day: bool,
    ) {
        let Some(raw_value) = object
            .get(key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .map(str::to_owned)
        else {
            return;
        };

        if raw_value.is_empty() {
            object.remove(key);
            return;
        }

        if let Some(parsed) = Self::parse_datetime(raw_value.as_str()) {
            object.insert(
                key.to_string(),
                serde_json::Value::String(parsed.to_rfc3339()),
            );
            return;
        }

        let Ok(date_only) = NaiveDate::parse_from_str(raw_value.as_str(), "%Y-%m-%d") else {
            return;
        };

        let naive_dt = if end_of_day {
            date_only.and_hms_opt(23, 59, 59)
        } else {
            date_only.and_hms_opt(0, 0, 0)
        };

        if let Some(naive) = naive_dt {
            let datetime = DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc);
            object.insert(
                key.to_string(),
                serde_json::Value::String(datetime.to_rfc3339()),
            );
        }
    }

    fn slice_by_char_range(content: &str, start: usize, end: usize) -> String {
        if start >= end {
            return String::new();
        }

        let mut start_byte = content.len();
        let mut end_byte = content.len();

        for (char_idx, (byte_idx, _)) in content.char_indices().enumerate() {
            if char_idx == start {
                start_byte = byte_idx;
            }
            if char_idx == end {
                end_byte = byte_idx;
                break;
            }
        }

        if start == 0 {
            start_byte = 0;
        }

        if end >= content.chars().count() {
            end_byte = content.len();
        }

        if start_byte >= end_byte || start_byte > content.len() || end_byte > content.len() {
            return String::new();
        }

        content[start_byte..end_byte].to_string()
    }

    fn build_document_result_from_search(&self, result: InfraSearchResult) -> DocumentResult {
        let filename = result
            .filename
            .or(result.file_name)
            .unwrap_or_else(|| "Unknown".to_string());
        let file_path = result.file_path.unwrap_or_default();
        let mime_type = result
            .mime_type
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let snippet = result.snippet.or(result.content).unwrap_or_default();
        let modified_at = result
            .updated_at
            .as_deref()
            .and_then(Self::parse_datetime)
            .unwrap_or_else(Utc::now);
        let size_bytes = result.size_bytes.unwrap_or(0);
        let document_id = result.document_id.or(result.file_id).unwrap_or(result.id);

        DocumentResult {
            document_id,
            filename,
            file_path,
            mime_type,
            score: result.score,
            snippet,
            chunk_index: result.chunk_index,
            modified_at,
            size_bytes,
        }
    }

    fn build_document_result_from_hybrid(&self, result: HybridSearchResult) -> DocumentResult {
        let metadata = result.metadata.as_ref().and_then(|value| value.as_object());

        let filename = metadata
            .and_then(|m| m.get("filename"))
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();
        let file_path = metadata
            .and_then(|m| m.get("path"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mime_type = metadata
            .and_then(|m| m.get("file_type"))
            .and_then(|v| v.as_str())
            .unwrap_or("application/octet-stream")
            .to_string();
        let size_bytes = metadata
            .and_then(|m| m.get("file_size"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let modified_at = metadata
            .and_then(|m| m.get("updated_at"))
            .and_then(|v| v.as_str())
            .and_then(Self::parse_datetime)
            .unwrap_or_else(Utc::now);
        let chunk_index = metadata
            .and_then(|m| m.get("chunk_index"))
            .and_then(|v| v.as_i64())
            .and_then(|v| usize::try_from(v).ok());

        DocumentResult {
            document_id: result.document_id.clone(),
            filename,
            file_path,
            mime_type,
            score: result.score,
            snippet: result.content.clone(),
            chunk_index,
            modified_at,
            size_bytes,
        }
    }

    fn build_document_evidence(results: &[DocumentResult]) -> Vec<DocumentEvidence> {
        let mut groups: Vec<DocumentEvidence> = Vec::new();
        let mut by_document: HashMap<String, usize> = HashMap::new();

        for result in results {
            let entry_index = if let Some(index) = by_document.get(&result.document_id) {
                *index
            } else {
                let index = groups.len();
                groups.push(DocumentEvidence {
                    document_id: result.document_id.clone(),
                    filename: result.filename.clone(),
                    file_path: result.file_path.clone(),
                    mime_type: result.mime_type.clone(),
                    max_score: result.score,
                    match_count: 0,
                    matches: Vec::new(),
                });
                by_document.insert(result.document_id.clone(), index);
                index
            };

            if let Some(group) = groups.get_mut(entry_index) {
                group.max_score = group.max_score.max(result.score);
                group.matches.push(DocumentEvidenceMatch {
                    chunk_index: result.chunk_index,
                    score: result.score,
                    excerpt: Self::truncate_excerpt(&result.snippet, 280),
                });
                group.match_count = group.matches.len();
            }
        }

        for group in &mut groups {
            group.matches.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.chunk_index.cmp(&b.chunk_index))
            });
            if group.matches.len() > 8 {
                group.matches.truncate(8);
                group.match_count = group.matches.len();
            }
        }

        groups.sort_by(|a, b| {
            b.max_score
                .partial_cmp(&a.max_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.match_count.cmp(&a.match_count))
                .then_with(|| a.filename.cmp(&b.filename))
        });

        groups
    }

    fn truncate_excerpt(text: &str, max_chars: usize) -> String {
        if text.chars().count() <= max_chars {
            return text.to_string();
        }

        let mut truncated: String = text.chars().take(max_chars).collect();
        truncated.push_str("...");
        truncated
    }

    /// Execute semantic_search function
    ///
    /// Boxed to prevent stack overflow: all handlers are boxed so the execute()
    /// match arms hold small Pin<Box<...>> pointers instead of inline futures.
    fn handle_semantic_search<'a>(
        &'a self,
        args: &'a serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FunctionResult>> + Send + 'a>>
    {
        Box::pin(async move {
            let start = Instant::now();

            // Deserialize and validate input
            let input: SemanticSearchInput = serde_json::from_value(args.clone())
                .map_err(|e| AppError::InvalidInput(format!("Invalid arguments: {}", e)))?;

            debug!(
                "Semantic search: query='{}', limit={}",
                input.query, input.limit
            );

            // Execute search based on mode
            let doc_results: Vec<DocumentResult> = match input.search_mode {
                SearchMode::Semantic => {
                    let embedding = self.embedding_service.embed_single(&input.query).await?;
                    let results = self
                        .search_service
                        .search_with_metadata(&embedding, input.limit)
                        .await?
                        .into_iter()
                        .filter(|r| r.score >= input.threshold)
                        .map(|r| self.build_document_result_from_search(r))
                        .collect::<Vec<_>>();
                    results
                }
                SearchMode::Keyword => self
                    .bm25_service
                    .search(&input.query, input.limit)
                    .await?
                    .into_iter()
                    .map(|r| self.build_document_result_from_search(r.into()))
                    .collect(),
                SearchMode::Hybrid => {
                    let embedding = self.embedding_service.embed_single(&input.query).await?;
                    let results = self
                        .hybrid_service
                        .search(
                            &input.query,
                            &embedding,
                            input.limit,
                            HybridSearchMode::Hybrid,
                        )
                        .await?
                        .into_iter()
                        .map(|r| self.build_document_result_from_hybrid(r))
                        .collect::<Vec<_>>();
                    results
                }
            };

            let search_time_ms = start.elapsed().as_secs_f64() * 1000.0;
            let documents = Self::build_document_evidence(&doc_results);

            let output = SemanticSearchOutput {
                results: doc_results.clone(),
                documents,
                total_found: doc_results.len(),
                search_time_ms,
                query: input.query,
            };

            info!(
                "Semantic search completed: {} results in {:.2}ms",
                output.total_found, search_time_ms
            );

            Ok(FunctionResult::success(serde_json::to_value(output)?))
        }) // Box::pin handle_semantic_search
    }

    /// Execute get_document function
    ///
    /// Boxed to prevent stack overflow: the Vec<Chunk> from find_by_document
    /// would inflate the parent execute() Future state machine.
    fn handle_get_document<'a>(
        &'a self,
        args: &'a serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FunctionResult>> + Send + 'a>>
    {
        Box::pin(async move {
            let input: GetDocumentInput = serde_json::from_value(args.clone())
                .map_err(|e| AppError::InvalidInput(format!("Invalid arguments: {}", e)))?;

            debug!("Get document: id='{}'", input.document_id);

            // Find document
            let doc = self
                .document_repository
                .find_by_id(&input.document_id)
                .await?
                .ok_or_else(|| {
                    AppError::NotFound(format!("Document '{}' not found", input.document_id))
                })?;

            // Read content: prefer extracted text chunks (works for PDFs, DOCX, etc.)
            // Fall back to raw file read only for plain text files
            let content = {
                let chunks = doc.chunks();
                debug!(
                    document_id = %input.document_id,
                    chunk_count = chunks.len(),
                    "Get document: using aggregate chunks"
                );

                if !chunks.is_empty() {
                    // Reassemble document text from indexed chunks (sorted by position)
                    let mut sorted_chunks: Vec<_> = chunks.iter().collect();
                    sorted_chunks.sort_by_key(|c| c.index());
                    sorted_chunks
                        .iter()
                        .map(|c| c.content())
                        .collect::<Vec<_>>()
                        .join("\n\n")
                } else {
                    // Fallback: try repository (legacy) then raw file
                    let repo_chunks = self
                        .chunk_repository
                        .find_by_document(doc.id().as_str())
                        .await
                        .unwrap_or_default();

                    if !repo_chunks.is_empty() {
                        debug!(
                            document_id = %input.document_id,
                            chunk_count = repo_chunks.len(),
                            "Get document: using repository chunks"
                        );
                        let mut sorted_chunks = repo_chunks;
                        sorted_chunks.sort_by_key(|c| c.index());
                        sorted_chunks
                            .iter()
                            .map(|c| c.content())
                            .collect::<Vec<_>>()
                            .join("\n\n")
                    } else {
                        debug!(
                            document_id = %input.document_id,
                            "Get document: using raw file fallback"
                        );
                        // No chunks — try reading the raw file (works for plain text)
                        self.file_storage
                            .read_file(doc.file_path())
                            .await
                            .unwrap_or_else(|e| {
                                tracing::warn!(
                                    document_id = %input.document_id,
                                    error = %e,
                                    "Failed to read file content, returning empty"
                                );
                                String::new()
                            })
                    }
                }
            };

            // Paginate content for large documents using max_content_length as page size.
            let page_size_chars = input.max_content_length.max(1);
            let total_chars = content.chars().count();
            let total_pages = std::cmp::max(1, total_chars.div_ceil(page_size_chars));
            let current_page = input.page.clamp(1, total_pages);
            let start_char = (current_page - 1) * page_size_chars;
            let end_char = std::cmp::min(start_char + page_size_chars, total_chars);
            let final_content = Self::slice_by_char_range(&content, start_char, end_char);
            let truncated = total_pages > 1;

            let has_previous_page = current_page > 1;
            let has_next_page = current_page < total_pages;
            let previous_page = has_previous_page.then_some(current_page - 1);
            let next_page = has_next_page.then_some(current_page + 1);
            debug!(
                document_id = %input.document_id,
                content_len = final_content.len(),
                total_chars = total_chars,
                page = current_page,
                total_pages = total_pages,
                truncated = truncated,
                "Get document: content assembled"
            );

            // Build metadata if requested
            let metadata = if input.include_metadata {
                debug!(
                    document_id = %input.document_id,
                    "Get document: building metadata"
                );
                let size_bytes = match self.file_storage.metadata(doc.file_path()).await {
                    Ok(meta) => meta.size as i64,
                    Err(e) => {
                        tracing::warn!(
                            document_id = %input.document_id,
                            error = %e,
                            "Failed to read file metadata, using stored size"
                        );
                        doc.size_bytes()
                    }
                };

                let tags = match self
                    .tag_service
                    .get_tags_for_document(doc.id().as_str())
                    .await
                {
                    Ok(tags) => tags
                        .into_iter()
                        .map(|tag| tag.name().as_str().to_string())
                        .collect(),
                    Err(e) => {
                        tracing::warn!(
                            document_id = %input.document_id,
                            error = %e,
                            "Failed to fetch tags for document"
                        );
                        Vec::new()
                    }
                };
                debug!(
                    document_id = %input.document_id,
                    tag_count = tags.len(),
                    "Get document: tags loaded"
                );

                let chunk_count = match self
                    .chunk_repository
                    .count_by_document(doc.id().as_str())
                    .await
                {
                    Ok(count) => count.max(0) as usize,
                    Err(e) => {
                        tracing::warn!(
                            document_id = %input.document_id,
                            error = %e,
                            "Failed to fetch chunk count for document"
                        );
                        0
                    }
                };
                debug!(
                    document_id = %input.document_id,
                    chunk_count = chunk_count,
                    "Get document: chunk count loaded"
                );

                Some(DocumentMetadata {
                    filename: doc
                        .file_path()
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    file_path: doc.file_path().display().to_string(),
                    mime_type: doc.mime_type().to_string(),
                    extension: doc
                        .file_path()
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_string(),
                    size_bytes,
                    created_at: None,
                    modified_at: *doc.updated_at(),
                    indexed_at: *doc.indexed_at(),
                    tags,
                    chunk_count,
                })
            } else {
                None
            };

            let output = GetDocumentOutput {
                document_id: input.document_id,
                content: final_content,
                content_truncated: truncated,
                page: current_page,
                total_pages,
                total_chars,
                has_previous_page,
                has_next_page,
                previous_page,
                next_page,
                metadata,
            };

            info!("Get document completed: id='{}'", output.document_id);

            Ok(FunctionResult::success(serde_json::to_value(output)?))
        }) // Box::pin(async move)
    }

    /// Execute list_documents function
    ///
    /// Boxed to prevent stack overflow in execute() match dispatch.
    fn handle_list_documents<'a>(
        &'a self,
        args: &'a serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FunctionResult>> + Send + 'a>>
    {
        Box::pin(async move {
            let input: ListDocumentsInput = serde_json::from_value(args.clone())
                .map_err(|e| AppError::InvalidInput(format!("Invalid arguments: {}", e)))?;

            debug!(
                "List documents: filter={:?}, limit={}",
                input.filter_mode, input.limit
            );

            let mut documents = self.document_repository.find_all().await?;

            let favorite_ids: HashSet<String> = self
                .favorites_repository
                .list_favorites()
                .await?
                .into_iter()
                .map(|favorite| favorite.document_id)
                .collect();

            let recent_limit = input.limit.saturating_add(input.offset);
            let recent_ids: HashSet<String> = if matches!(input.filter_mode, FilterMode::Recent) {
                self.recent_documents_repository
                    .get_recent_documents(recent_limit)
                    .await?
                    .into_iter()
                    .map(|recent| recent.document_id)
                    .collect()
            } else {
                HashSet::new()
            };

            let tag_ids: HashSet<String> = if matches!(input.filter_mode, FilterMode::ByTag) {
                match &input.tags {
                    Some(tags) => {
                        let mut ids = HashSet::new();
                        for tag in tags {
                            for document_id in self.tag_service.search_documents_by_tag(tag).await?
                            {
                                ids.insert(document_id);
                            }
                        }
                        ids
                    }
                    None => HashSet::new(),
                }
            } else {
                HashSet::new()
            };

            let file_types = input.file_types.as_ref().map(|types| {
                types
                    .iter()
                    .map(|t| t.to_lowercase())
                    .collect::<HashSet<_>>()
            });

            documents.retain(|doc| {
                let document_id = doc.id().as_str();

                match input.filter_mode {
                    FilterMode::Recent if !recent_ids.contains(document_id) => return false,
                    FilterMode::Favorites if !favorite_ids.contains(document_id) => return false,
                    FilterMode::ByTag if !tag_ids.contains(document_id) => return false,
                    _ => {}
                }

                if let Some(types) = &file_types {
                    let extension = doc
                        .file_path()
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    if !types.contains(&extension) {
                        return false;
                    }
                }

                if let Some(date_from) = &input.date_from {
                    if doc.modified_at() < date_from {
                        return false;
                    }
                }

                if let Some(date_to) = &input.date_to {
                    if doc.modified_at() > date_to {
                        return false;
                    }
                }

                true
            });

            documents.sort_by(|left, right| {
                let ordering = match input.sort_by {
                    SortField::Modified => left.modified_at().cmp(right.modified_at()),
                    SortField::Created => left.indexed_at().cmp(right.indexed_at()),
                    SortField::Name => left.file_name().cmp(right.file_name()),
                    SortField::Size => left.size_bytes().cmp(&right.size_bytes()),
                };

                match input.sort_order {
                    SortOrder::Asc => ordering,
                    SortOrder::Desc => ordering.reverse(),
                }
            });

            let total = documents.len();
            let start = input.offset.min(total);
            let end = (start + input.limit).min(total);
            let page = documents.into_iter().skip(start).take(end - start);

            let mut items = Vec::new();
            for doc in page {
                let document_id = doc.id().as_str().to_string();
                let tags = self
                    .tag_service
                    .get_tags_for_document(doc.id().as_str())
                    .await?
                    .into_iter()
                    .map(|tag| tag.name().as_str().to_string())
                    .collect();

                let extension = doc
                    .file_path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or("")
                    .to_string();

                items.push(DocumentListItem {
                    document_id: document_id.clone(),
                    filename: doc.file_name().to_string(),
                    file_path: doc.file_path().display().to_string(),
                    mime_type: doc.mime_type().to_string(),
                    extension,
                    size_bytes: doc.size_bytes(),
                    modified_at: *doc.modified_at(),
                    tags,
                    is_favorite: favorite_ids.contains(&document_id),
                    access_count: doc.access_count().max(0) as usize,
                });
            }

            let output = ListDocumentsOutput {
                documents: items,
                total,
                limit: input.limit,
                offset: input.offset,
                has_more: end < total,
            };

            info!("List documents completed: {} documents", output.total);

            Ok(FunctionResult::success(serde_json::to_value(output)?))
        }) // Box::pin handle_list_documents
    }

    /// Execute web_search function (Phase 2)
    ///
    /// Boxed to prevent stack overflow in execute() match dispatch.
    fn handle_web_search<'a>(
        &'a self,
        args: &'a serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FunctionResult>> + Send + 'a>>
    {
        Box::pin(async move {
            let input: WebSearchInput = serde_json::from_value(args.clone())
                .map_err(|e| AppError::InvalidInput(format!("Invalid arguments: {}", e)))?;

            debug!(
                "Web search: query='{}', max_results={}, page={}, offset={}, providers={:?}, include_wikipedia={}, depth={}, branch_queries={}",
                input.query,
                input.max_results,
                input.page,
                input.offset,
                input.providers,
                input.include_wikipedia,
                input.depth,
                input.branch_queries
            );

            // Execute web search
            let output = self.web_service.search_web(&input).await?;

            info!("Web search completed: {} results", output.result_count);

            Ok(FunctionResult::success(serde_json::to_value(output)?))
        }) // Box::pin handle_web_search
    }

    /// Execute fetch_url_content function (Phase 2)
    ///
    /// Boxed to prevent stack overflow in execute() match dispatch.
    fn handle_fetch_url<'a>(
        &'a self,
        args: &'a serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FunctionResult>> + Send + 'a>>
    {
        Box::pin(async move {
            let input: FetchUrlContentInput = serde_json::from_value(args.clone())
                .map_err(|e| AppError::InvalidInput(format!("Invalid arguments: {}", e)))?;

            debug!("Fetch URL: url='{}'", input.url);

            // Fetch and extract content
            let output = self.web_service.fetch_url_content(&input.url).await?;

            info!(
                "Fetch URL completed: {} words in {:.2}ms",
                output.word_count, output.fetch_time_ms
            );

            Ok(FunctionResult::success(serde_json::to_value(output)?))
        }) // Box::pin handle_fetch_url
    }

    fn strip_html_tags(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut in_tag = false;
        for ch in input.chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => out.push(ch),
                _ => {}
            }
        }
        out.replace("&quot;", "\"")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&#39;", "'")
    }

    /// Execute wiki_search function.
    fn handle_wiki_search<'a>(
        &'a self,
        args: &'a serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FunctionResult>> + Send + 'a>>
    {
        Box::pin(async move {
            let input: WikiSearchInput = serde_json::from_value(args.clone())
                .map_err(|e| AppError::InvalidInput(format!("Invalid arguments: {}", e)))?;

            debug!(
                "Wikipedia search: query='{}', max_results={}",
                input.query, input.max_results
            );

            let limit = input.max_results.clamp(1, 10);
            let api_url = format!(
                "https://en.wikipedia.org/w/api.php?action=query&format=json&utf8=1&list=search&srsearch={}&srlimit={}&srprop=snippet",
                urlencoding::encode(input.query.trim()),
                limit
            );

            self.web_service.validate_url(&api_url)?;

            let response = self
                .http_client
                .get(&api_url)
                .header(ACCEPT, "application/json")
                .header(ACCEPT_LANGUAGE, "en-US,en;q=0.9")
                .header(USER_AGENT, WIKIPEDIA_USER_AGENT)
                .send()
                .await
                .map_err(|e| AppError::Network(format!("Failed to call Wikipedia API: {}", e)))?;

            if !response.status().is_success() {
                return Err(AppError::Network(format!(
                    "Wikipedia API returned HTTP {}",
                    response.status()
                )));
            }

            let payload = response
                .json::<serde_json::Value>()
                .await
                .map_err(|e| AppError::Network(format!("Invalid Wikipedia API response: {}", e)))?;

            let mut results = Vec::new();
            if let Some(items) = payload
                .get("query")
                .and_then(|v| v.get("search"))
                .and_then(|v| v.as_array())
            {
                for item in items.iter().take(limit) {
                    let title = item
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if title.is_empty() {
                        continue;
                    }
                    let snippet_raw = item
                        .get("snippet")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim();
                    let snippet = Self::strip_html_tags(snippet_raw)
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    let url = format!("https://en.wikipedia.org/wiki/{}", title.replace(' ', "_"));

                    results.push(WikiSearchResult {
                        title,
                        url,
                        snippet,
                    });
                }
            }

            let output = WikiSearchOutput {
                query: input.query,
                result_count: results.len(),
                results,
            };

            Ok(FunctionResult::success(serde_json::to_value(output)?))
        })
    }

    /// Execute wiki_summary function.
    fn handle_wiki_summary<'a>(
        &'a self,
        args: &'a serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FunctionResult>> + Send + 'a>>
    {
        Box::pin(async move {
            let input: WikiSummaryInput = serde_json::from_value(args.clone())
                .map_err(|e| AppError::InvalidInput(format!("Invalid arguments: {}", e)))?;

            let title = input.title.trim();
            if title.is_empty() {
                return Err(AppError::InvalidInput(
                    "Wikipedia title cannot be empty".to_string(),
                ));
            }

            debug!("Wikipedia summary: title='{}'", title);

            let encoded_title = title.replace(' ', "_");
            let api_url = format!(
                "https://en.wikipedia.org/api/rest_v1/page/summary/{}",
                urlencoding::encode(&encoded_title)
            );

            self.web_service.validate_url(&api_url)?;

            let response = self
                .http_client
                .get(&api_url)
                .header(ACCEPT, "application/json")
                .header(ACCEPT_LANGUAGE, "en-US,en;q=0.9")
                .header(USER_AGENT, WIKIPEDIA_USER_AGENT)
                .send()
                .await
                .map_err(|e| AppError::Network(format!("Failed to call Wikipedia API: {}", e)))?;

            if !response.status().is_success() {
                return Err(AppError::Network(format!(
                    "Wikipedia summary returned HTTP {}",
                    response.status()
                )));
            }

            let payload = response
                .json::<serde_json::Value>()
                .await
                .map_err(|e| AppError::Network(format!("Invalid Wikipedia API response: {}", e)))?;

            let page_title = payload
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or(title)
                .to_string();
            let page_url = payload
                .get("content_urls")
                .and_then(|v| v.get("desktop"))
                .and_then(|v| v.get("page"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let extract = payload
                .get("extract")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let language = payload
                .get("lang")
                .and_then(|v| v.as_str())
                .unwrap_or("en")
                .to_string();

            let canonical_url = if page_url.is_empty() {
                format!("https://en.wikipedia.org/wiki/{}", encoded_title)
            } else {
                page_url
            };

            let output = WikiSummaryOutput {
                title: page_title,
                url: canonical_url,
                extract,
                language,
            };

            Ok(FunctionResult::success(serde_json::to_value(output)?))
        })
    }

    /// Execute a configured custom query tool.
    fn handle_custom_query_tool<'a>(
        &'a self,
        tool: &'a CustomToolSettingsDto,
        args: &'a serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FunctionResult>> + Send + 'a>>
    {
        Box::pin(async move {
            let input: CustomQueryToolInput = serde_json::from_value(args.clone())
                .map_err(|e| AppError::InvalidInput(format!("Invalid arguments: {}", e)))?;

            let mut parsed_url = Url::parse(tool.endpoint.trim()).map_err(|e| {
                AppError::InvalidInput(format!(
                    "Invalid endpoint for custom tool '{}': {}",
                    tool.name, e
                ))
            })?;

            let requested_max_results = args
                .get("max_results")
                .and_then(|v| v.as_u64())
                .unwrap_or(tool.default_max_results as u64)
                .max(1);

            {
                let mut query_pairs = parsed_url.query_pairs_mut();
                query_pairs.append_pair(tool.query_param.trim(), input.query.trim());
                if let Some(max_results_param) = &tool.max_results_param {
                    query_pairs
                        .append_pair(max_results_param.trim(), &requested_max_results.to_string());
                }
            }

            let request_url = parsed_url.to_string();
            self.web_service.validate_url(&request_url)?;

            let response = self
                .http_client
                .get(&request_url)
                .header(ACCEPT, "application/json, text/plain;q=0.9, */*;q=0.8")
                .header(ACCEPT_LANGUAGE, "en-US,en;q=0.9")
                .send()
                .await
                .map_err(|e| {
                    AppError::Network(format!("Custom tool '{}' request failed: {}", tool.name, e))
                })?;

            if !response.status().is_success() {
                return Err(AppError::Network(format!(
                    "Custom tool '{}' returned HTTP {}",
                    tool.name,
                    response.status()
                )));
            }

            let payload = response.json::<serde_json::Value>().await.map_err(|e| {
                AppError::Network(format!(
                    "Custom tool '{}' returned non-JSON payload: {}",
                    tool.name, e
                ))
            })?;

            let output = CustomQueryToolOutput {
                tool_name: tool.name.clone(),
                request_url,
                data: payload,
            };

            Ok(FunctionResult::success(serde_json::to_value(output)?))
        })
    }

    fn validate_custom_query_arguments(arguments: &serde_json::Value) -> Result<()> {
        let object = arguments
            .as_object()
            .ok_or_else(|| AppError::InvalidInput("Arguments must be a JSON object".to_string()))?;

        let query = object
            .get("query")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|query| !query.is_empty())
            .ok_or_else(|| AppError::InvalidInput("Missing required field 'query'".to_string()))?;

        if query.is_empty() {
            return Err(AppError::InvalidInput(
                "Missing required field 'query'".to_string(),
            ));
        }

        if let Some(max_results) = object.get("max_results") {
            let as_u64 = max_results
                .as_u64()
                .or_else(|| {
                    max_results
                        .as_i64()
                        .and_then(|value| u64::try_from(value).ok())
                })
                .or_else(|| {
                    max_results.as_f64().and_then(|value| {
                        let integer = value.trunc();
                        if (value - integer).abs() <= f64::EPSILON && integer >= 0.0 {
                            Some(integer as u64)
                        } else {
                            None
                        }
                    })
                });

            match as_u64 {
                Some(value) if (1..=100).contains(&value) => {}
                _ => {
                    return Err(AppError::InvalidInput(
                        "Field 'max_results' must be an integer between 1 and 100".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }
}

#[async_trait]
impl FunctionExecutorTrait for FunctionExecutor {
    async fn execute(&self, call: FunctionCall) -> Result<FunctionResult> {
        info!(
            "Executing function call: name='{}', id='{}'",
            call.name, call.id
        );

        let is_registered = self.registry.get_tool(&call.name).is_some();
        let has_custom_handler = self
            .custom_tools
            .read()
            .ok()
            .is_some_and(|tools| tools.contains_key(call.name.as_str()));

        // Verify function exists
        if !is_registered && !has_custom_handler {
            error!("Function '{}' not found in registry", call.name);
            return Ok(FunctionResult::error(
                "FUNCTION_NOT_FOUND",
                format!("Function '{}' is not registered", call.name),
            ));
        }

        let normalized_arguments = Self::normalize_tool_arguments(&call.name, &call.arguments);

        // Validate arguments
        if let Err(e) = self.validate_arguments(&call.name, &normalized_arguments) {
            error!("Invalid arguments for '{}': {}", call.name, e);
            return Ok(FunctionResult::error("INVALID_ARGUMENTS", e.to_string()));
        }

        // Route to appropriate handler
        let result = match call.name.as_str() {
            TOOL_SEMANTIC_SEARCH => self.handle_semantic_search(&normalized_arguments).await,
            TOOL_GET_DOCUMENT => self.handle_get_document(&normalized_arguments).await,
            TOOL_LIST_DOCUMENTS => self.handle_list_documents(&normalized_arguments).await,
            TOOL_WEB_SEARCH => self.handle_web_search(&normalized_arguments).await,
            TOOL_FETCH_URL_CONTENT => self.handle_fetch_url(&normalized_arguments).await,
            TOOL_WIKI_SEARCH => self.handle_wiki_search(&normalized_arguments).await,
            TOOL_WIKI_SUMMARY => self.handle_wiki_summary(&normalized_arguments).await,
            _ => match self
                .custom_tools
                .read()
                .ok()
                .and_then(|tools| tools.get(call.name.as_str()).cloned())
            {
                Some(custom_tool) => {
                    self.handle_custom_query_tool(&custom_tool, &normalized_arguments)
                        .await
                }
                None => {
                    error!("No handler for function '{}'", call.name);
                    return Ok(FunctionResult::error(
                        "NO_HANDLER",
                        format!("No handler implemented for function '{}'", call.name),
                    ));
                }
            },
        };

        match result {
            Ok(function_result) => {
                info!("Function '{}' executed successfully", call.name);
                Ok(function_result)
            }
            Err(e) => {
                error!("Function '{}' failed: {}", call.name, e);
                Ok(FunctionResult::error("EXECUTION_ERROR", e.to_string()))
            }
        }
    }

    fn set_custom_tools(&self, custom_tools: HashMap<String, CustomToolSettingsDto>) {
        if let Ok(mut guard) = self.custom_tools.write() {
            *guard = custom_tools;
        }
    }

    fn validate_arguments(&self, function_name: &str, arguments: &serde_json::Value) -> Result<()> {
        if let Some(tool) = self.registry.get_tool(function_name) {
            // Basic validation: check that arguments is an object
            if !arguments.is_object() {
                return Err(AppError::InvalidInput(
                    "Arguments must be a JSON object".to_string(),
                ));
            }

            let schema = JSONSchema::compile(&tool.input_schema).map_err(|e| {
                AppError::InvalidInput(format!("Invalid schema for '{}': {}", function_name, e))
            })?;

            if let Err(errors) = schema.validate(arguments) {
                let message = errors
                    .into_iter()
                    .next()
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "Schema validation failed".to_string());
                return Err(AppError::InvalidInput(message));
            }

            return Ok(());
        }

        if self
            .custom_tools
            .read()
            .ok()
            .is_some_and(|tools| tools.contains_key(function_name))
        {
            return Self::validate_custom_query_arguments(arguments);
        }

        Err(AppError::InvalidData(format!(
            "Function '{}' not found",
            function_name
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::dtos::favorite_dto::FavoriteDto;
    use crate::application::dtos::recent_dto::RecentDocumentDto;
    use crate::application::ports::DocumentRepositoryPort;
    use crate::application::ports::{
        ChunkRepositoryPort, FavoritesRepositoryPort, FileMetadata, FileStoragePort,
        RecentDocumentsRepositoryPort,
    };
    use crate::domain::entities::Document;
    use crate::domain::repositories::mocks::DddMockDocumentRepository as DddMockDocRepo;
    use crate::domain::value_objects::Checksum;
    use crate::infrastructure::persistence::repositories::mocks::MockChunkRepository;
    use crate::infrastructure::services::function_registry::FunctionRegistry;
    use crate::infrastructure::services::mocks::{
        MockBM25Search, MockEmbeddingService, MockFunctionRegistry, MockHybridSearch,
        MockSearchService, MockTagService, MockWebService,
    };
    use crate::RepositoryPort;
    use async_trait::async_trait;
    use std::path::{Path, PathBuf};

    struct MockFileStoragePort;

    struct MockFavoritesRepository;

    impl MockFavoritesRepository {
        fn new() -> Self {
            Self
        }
    }

    #[async_trait]
    impl FavoritesRepositoryPort for MockFavoritesRepository {
        async fn add_favorite(&self, document_id: &str) -> Result<FavoriteDto> {
            Ok(FavoriteDto {
                id: "fav-1".to_string(),
                document_id: document_id.to_string(),
                document_name: "Test Document".to_string(),
                document_path: "/path/to/doc".to_string(),
                file_type: None,
                added_at: "2024-01-01T00:00:00Z".to_string(),
            })
        }

        async fn remove_favorite(&self, _document_id: &str) -> Result<()> {
            Ok(())
        }

        async fn list_favorites(&self) -> Result<Vec<FavoriteDto>> {
            Ok(Vec::new())
        }

        async fn is_favorite(&self, _document_id: &str) -> Result<bool> {
            Ok(false)
        }
    }

    struct MockRecentDocumentsRepository;

    impl MockRecentDocumentsRepository {
        fn new() -> Self {
            Self
        }
    }

    #[async_trait]
    impl RecentDocumentsRepositoryPort for MockRecentDocumentsRepository {
        async fn track_access(&self, _document_id: &str) -> Result<()> {
            Ok(())
        }

        async fn get_recent_documents(&self, _limit: usize) -> Result<Vec<RecentDocumentDto>> {
            Ok(Vec::new())
        }

        async fn clear_recent_history(&self, _before_date: Option<&str>) -> Result<usize> {
            Ok(0)
        }
    }

    #[async_trait]
    impl FileStoragePort for MockFileStoragePort {
        async fn read_file(&self, _path: &Path) -> Result<String> {
            Ok(String::new())
        }

        async fn read_file_bytes(&self, _path: &Path) -> Result<Vec<u8>> {
            Ok(Vec::new())
        }

        async fn write_file(&self, _path: &Path, _content: &str) -> Result<()> {
            Ok(())
        }

        async fn write_file_bytes(&self, _path: &Path, _content: &[u8]) -> Result<()> {
            Ok(())
        }

        async fn delete_file(&self, _path: &Path) -> Result<()> {
            Ok(())
        }

        async fn compute_hash(&self, _path: &Path) -> Result<String> {
            Ok("".to_string())
        }

        async fn exists(&self, _path: &Path) -> bool {
            false
        }

        async fn metadata(&self, _path: &Path) -> Result<FileMetadata> {
            Ok(FileMetadata {
                size: 0,
                modified_at: 0,
                is_file: true,
                is_directory: false,
            })
        }
    }

    struct LongContentFileStorage {
        content: String,
    }

    #[async_trait]
    impl FileStoragePort for LongContentFileStorage {
        async fn read_file(&self, _path: &Path) -> Result<String> {
            Ok(self.content.clone())
        }

        async fn read_file_bytes(&self, _path: &Path) -> Result<Vec<u8>> {
            Ok(self.content.as_bytes().to_vec())
        }

        async fn write_file(&self, _path: &Path, _content: &str) -> Result<()> {
            Ok(())
        }

        async fn write_file_bytes(&self, _path: &Path, _content: &[u8]) -> Result<()> {
            Ok(())
        }

        async fn delete_file(&self, _path: &Path) -> Result<()> {
            Ok(())
        }

        async fn compute_hash(&self, _path: &Path) -> Result<String> {
            Ok("".to_string())
        }

        async fn exists(&self, _path: &Path) -> bool {
            true
        }

        async fn metadata(&self, _path: &Path) -> Result<FileMetadata> {
            Ok(FileMetadata {
                size: self.content.len() as u64,
                modified_at: 0,
                is_file: true,
                is_directory: false,
            })
        }
    }

    struct SingleDocumentRepository {
        document: Document,
    }

    impl SingleDocumentRepository {
        fn new(document: Document) -> Self {
            Self { document }
        }
    }

    #[async_trait]
    impl RepositoryPort<Document> for SingleDocumentRepository {
        async fn find_by_id(&self, id: &str) -> Result<Option<Document>> {
            if self.document.id().as_str() == id {
                Ok(Some(self.document.clone()))
            } else {
                Ok(None)
            }
        }

        async fn find_by_filter(
            &self,
            _filter: &dyn crate::application::ports::repository_port::Filter,
        ) -> Result<Vec<Document>> {
            Ok(vec![self.document.clone()])
        }

        async fn find_all(&self) -> Result<Vec<Document>> {
            Ok(vec![self.document.clone()])
        }

        async fn save(&self, _entity: &Document) -> Result<()> {
            Ok(())
        }

        async fn save_batch(&self, _entities: &[Document]) -> Result<()> {
            Ok(())
        }

        async fn delete(&self, _id: &str) -> Result<()> {
            Ok(())
        }

        async fn delete_batch(&self, _ids: &[&str]) -> Result<()> {
            Ok(())
        }

        async fn count(&self) -> Result<usize> {
            Ok(1)
        }

        async fn exists(&self, id: &str) -> Result<bool> {
            Ok(self.document.id().as_str() == id)
        }
    }

    #[async_trait]
    impl DocumentRepositoryPort for SingleDocumentRepository {
        async fn find_file_path_by_id(&self, document_id: &str) -> Result<String> {
            if self.document.id().as_str() == document_id {
                Ok(self.document.file_path().display().to_string())
            } else {
                Err(AppError::NotFound(format!(
                    "Document not found: {}",
                    document_id
                )))
            }
        }

        async fn document_exists(&self, document_id: &str) -> Result<bool> {
            Ok(self.document.id().as_str() == document_id)
        }

        async fn find_id_by_path(&self, file_path: &str) -> Result<Option<String>> {
            let matches = self.document.file_path().display().to_string() == file_path;
            Ok(matches.then(|| self.document.id().as_str().to_string()))
        }

        async fn delete(&self, _document_id: &str) -> Result<()> {
            Ok(())
        }

        async fn find_by_checksum(&self, checksum: &Checksum) -> Result<Option<Document>> {
            Ok((self.document.checksum() == checksum).then(|| self.document.clone()))
        }

        async fn count_documents(&self) -> Result<i64> {
            Ok(1)
        }

        async fn count_chunks(&self) -> Result<i64> {
            Ok(self.document.chunks().len() as i64)
        }
    }

    impl crate::application::ports::DocumentRepository for SingleDocumentRepository {}

    #[tokio::test]
    async fn test_execute_unknown_function() {
        let registry = Arc::new(MockFunctionRegistry::new());
        let embedding = Arc::new(MockEmbeddingService::default());
        let search = Arc::new(MockSearchService::new());
        let bm25 = Arc::new(MockBM25Search::new());
        let hybrid = Arc::new(MockHybridSearch::new());
        let mock = Arc::new(DddMockDocRepo::new());
        use crate::application::ports::DocumentRepository;
        let doc_repo = mock as Arc<dyn DocumentRepository>;
        let chunk_repo = Arc::new(MockChunkRepository::new()) as Arc<dyn ChunkRepositoryPort>;
        let tag_service = Arc::new(MockTagService::new()) as Arc<dyn TagServiceTrait>;
        let file_storage = Arc::new(MockFileStoragePort) as Arc<dyn FileStoragePort>;
        let web = Arc::new(MockWebService::new());
        let favorites_repository = Arc::new(MockFavoritesRepository::new());
        let recent_documents_repository = Arc::new(MockRecentDocumentsRepository::new());

        let executor = FunctionExecutor::new(
            registry as Arc<dyn FunctionRegistryTrait>,
            embedding as Arc<dyn EmbeddingServiceTrait>,
            search as Arc<dyn SearchServiceTrait>,
            bm25 as Arc<dyn BM25SearchTrait>,
            hybrid as Arc<dyn HybridSearchTrait>,
            doc_repo,
            chunk_repo,
            tag_service,
            favorites_repository,
            recent_documents_repository,
            file_storage,
            web as Arc<dyn WebServiceTrait>,
        );

        let call = FunctionCall::new("call_123", "unknown_function", serde_json::json!({}));

        let result = executor.execute(call).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.error_code.as_ref().unwrap(), "FUNCTION_NOT_FOUND");
    }

    #[tokio::test]
    async fn test_validate_arguments() {
        use crate::domain::function_call::ToolDefinition;
        use serde_json::json;

        let registry = Arc::new(MockFunctionRegistry::new());
        let embedding = Arc::new(MockEmbeddingService::default());
        let tool = ToolDefinition::new(
            "test_function",
            "Test function",
            json!({
                "type": "object",
                "properties": {
                    "required_param": {"type": "string"}
                },
                "required": ["required_param"]
            }),
        )
        .unwrap();
        registry.register(tool).unwrap();

        let search = Arc::new(MockSearchService::new());
        let bm25 = Arc::new(MockBM25Search::new());
        let hybrid = Arc::new(MockHybridSearch::new());
        let mock = Arc::new(DddMockDocRepo::new());
        use crate::application::ports::DocumentRepository;
        let doc_repo = mock as Arc<dyn DocumentRepository>;
        let chunk_repo = Arc::new(MockChunkRepository::new()) as Arc<dyn ChunkRepositoryPort>;
        let tag_service = Arc::new(MockTagService::new()) as Arc<dyn TagServiceTrait>;
        let favorites_repository = Arc::new(MockFavoritesRepository::new());
        let recent_documents_repository = Arc::new(MockRecentDocumentsRepository::new());
        let file_storage = Arc::new(MockFileStoragePort) as Arc<dyn FileStoragePort>;
        let web = Arc::new(MockWebService::new());

        let executor = FunctionExecutor::new(
            registry as Arc<dyn FunctionRegistryTrait>,
            embedding as Arc<dyn EmbeddingServiceTrait>,
            search as Arc<dyn SearchServiceTrait>,
            bm25 as Arc<dyn BM25SearchTrait>,
            hybrid as Arc<dyn HybridSearchTrait>,
            doc_repo,
            chunk_repo,
            tag_service,
            favorites_repository,
            recent_documents_repository,
            file_storage,
            web as Arc<dyn WebServiceTrait>,
        );

        // Valid arguments
        let valid = json!({"required_param": "value"});
        assert!(executor.validate_arguments("test_function", &valid).is_ok());

        // Missing required field
        let invalid = json!({});
        assert!(executor
            .validate_arguments("test_function", &invalid)
            .is_err());
    }

    #[test]
    fn test_normalize_tool_arguments_date_only_filters() {
        use chrono::Timelike;
        use serde_json::json;

        let args = json!({
            "query": "bigtime handbook",
            "date_from": "2000-01-01",
            "date_to": "2000-01-01"
        });

        let normalized = FunctionExecutor::normalize_tool_arguments("semantic_search", &args);

        let from = normalized
            .get("date_from")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let to = normalized
            .get("date_to")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let parsed_from = DateTime::parse_from_rfc3339(from).ok();
        let parsed_to = DateTime::parse_from_rfc3339(to).ok();

        assert!(parsed_from.is_some());
        assert!(parsed_to.is_some());

        if let Some(dt) = parsed_from {
            let utc = dt.with_timezone(&Utc);
            assert_eq!(utc.hour(), 0);
            assert_eq!(utc.minute(), 0);
            assert_eq!(utc.second(), 0);
        }

        if let Some(dt) = parsed_to {
            let utc = dt.with_timezone(&Utc);
            assert_eq!(utc.hour(), 23);
            assert_eq!(utc.minute(), 59);
            assert_eq!(utc.second(), 59);
        }
    }

    #[test]
    fn test_normalize_tool_arguments_removes_empty_datetime_filters() {
        use serde_json::json;

        let args = json!({
            "query": "arugula bitterness",
            "date_from": "   ",
            "date_to": ""
        });

        let normalized = FunctionExecutor::normalize_tool_arguments("semantic_search", &args);

        assert!(normalized.get("date_from").is_none());
        assert!(normalized.get("date_to").is_none());
    }

    #[tokio::test]
    async fn test_get_document_supports_internal_pagination() {
        use crate::application::ports::DocumentRepository;
        use crate::domain::function_call::ToolDefinition;
        use crate::shared::domain_types::ValidatedFilePath;
        use serde_json::json;

        let registry = Arc::new(FunctionRegistry::new());
        registry
            .register(
                ToolDefinition::new(
                    "get_document",
                    "Get document",
                    json!({
                        "type": "object",
                        "properties": {
                            "document_id": {"type": "string"},
                            "include_metadata": {"type": "boolean"},
                            "max_content_length": {
                                "type": "integer",
                                "minimum": 1000,
                                "maximum": 100000
                            },
                            "page": {
                                "type": "integer",
                                "minimum": 1
                            }
                        },
                        "required": ["document_id"]
                    }),
                )
                .expect("tool schema should be valid"),
            )
            .expect("tool should register");

        let embedding = Arc::new(MockEmbeddingService::default());
        let search = Arc::new(MockSearchService::new());
        let bm25 = Arc::new(MockBM25Search::new());
        let hybrid = Arc::new(MockHybridSearch::new());
        let chunk_repo = Arc::new(MockChunkRepository::new()) as Arc<dyn ChunkRepositoryPort>;
        let tag_service = Arc::new(MockTagService::new()) as Arc<dyn TagServiceTrait>;
        let favorites_repository = Arc::new(MockFavoritesRepository::new());
        let recent_documents_repository = Arc::new(MockRecentDocumentsRepository::new());
        let web = Arc::new(MockWebService::new()) as Arc<dyn WebServiceTrait>;

        let file_path = ValidatedFilePath::new(PathBuf::from("/tmp/pagination-test.txt"))
            .expect("file path should validate");
        let checksum = Checksum::new("a".repeat(64)).expect("checksum should validate");
        let document = Document::new(
            file_path,
            "pagination-test.txt".to_string(),
            "text/plain".to_string(),
            0,
            checksum,
        );
        let document_id = document.id().as_str().to_string();
        let doc_repo =
            Arc::new(SingleDocumentRepository::new(document)) as Arc<dyn DocumentRepository>;

        let file_storage = Arc::new(LongContentFileStorage {
            content: "A".repeat(2500),
        }) as Arc<dyn FileStoragePort>;

        let executor = FunctionExecutor::new(
            registry as Arc<dyn FunctionRegistryTrait>,
            embedding as Arc<dyn EmbeddingServiceTrait>,
            search as Arc<dyn SearchServiceTrait>,
            bm25 as Arc<dyn BM25SearchTrait>,
            hybrid as Arc<dyn HybridSearchTrait>,
            doc_repo,
            chunk_repo,
            tag_service,
            favorites_repository,
            recent_documents_repository,
            file_storage,
            web,
        );

        let call = FunctionCall::new(
            "call_get_doc",
            "get_document",
            json!({
                "document_id": document_id,
                "include_metadata": false,
                "max_content_length": 1000,
                "page": 2
            }),
        );

        let result = executor
            .execute(call)
            .await
            .expect("execution should succeed");
        assert!(result.success, "tool should succeed");

        let output: GetDocumentOutput = serde_json::from_value(result.data.expect("data expected"))
            .expect("output should deserialize");

        assert_eq!(output.page, 2);
        assert_eq!(output.total_pages, 3);
        assert_eq!(output.total_chars, 2500);
        assert_eq!(output.content.len(), 1000);
        assert!(output.has_previous_page);
        assert!(output.has_next_page);
        assert_eq!(output.previous_page, Some(1));
        assert_eq!(output.next_page, Some(3));
        assert!(output.content_truncated);
    }

    #[test]
    fn test_build_document_evidence_groups_by_document() {
        let now = Utc::now();
        let results = vec![
            DocumentResult {
                document_id: "doc-a".to_string(),
                filename: "a.pdf".to_string(),
                file_path: "/tmp/a.pdf".to_string(),
                mime_type: "application/pdf".to_string(),
                score: 0.91,
                snippet: "alpha excerpt".to_string(),
                chunk_index: Some(2),
                modified_at: now,
                size_bytes: 100,
            },
            DocumentResult {
                document_id: "doc-a".to_string(),
                filename: "a.pdf".to_string(),
                file_path: "/tmp/a.pdf".to_string(),
                mime_type: "application/pdf".to_string(),
                score: 0.77,
                snippet: "beta excerpt".to_string(),
                chunk_index: Some(5),
                modified_at: now,
                size_bytes: 100,
            },
            DocumentResult {
                document_id: "doc-b".to_string(),
                filename: "b.pdf".to_string(),
                file_path: "/tmp/b.pdf".to_string(),
                mime_type: "application/pdf".to_string(),
                score: 0.86,
                snippet: "gamma excerpt".to_string(),
                chunk_index: Some(1),
                modified_at: now,
                size_bytes: 100,
            },
        ];

        let grouped = FunctionExecutor::build_document_evidence(&results);
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0].document_id, "doc-a");
        assert_eq!(grouped[0].match_count, 2);
        assert!((grouped[0].max_score - 0.91).abs() < f32::EPSILON);
        assert_eq!(grouped[1].document_id, "doc-b");
        assert_eq!(grouped[1].match_count, 1);
    }
}
