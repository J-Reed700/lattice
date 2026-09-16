//! The `WebIngestionService` type and its end-to-end ingestion workflow.

use super::{WebIngestionConfig, WebIngestionServiceBuilder};
use crate::features::embedding::EmbeddingServiceTrait;
use crate::features::indexing::IndexStorageTrait;
use crate::features::web::{WebArchiveServiceTrait, WebIngestionResult, WebIngestionServiceTrait};
use crate::infrastructure::services::traits::ArticleExtractorServiceTrait;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokenizers::Tokenizer;

/// Web ingestion service
///
/// Production implementation that orchestrates the complete URL import workflow:
/// 1. Extracts article from URL (via ArticleExtractorService)
/// 2. Archives article as markdown file (via WebArchiveService)
/// 3. Chunks text content using the loaded embedding model input policy
/// 4. Generates embeddings (via EmbeddingService)
/// 5. Stores document with chunks and embeddings (via IndexStorage)
///
/// # Dependencies
///
/// - `ArticleExtractorServiceTrait`: Extracts clean article content from URLs
/// - `WebArchiveServiceTrait`: Saves articles as markdown files in ~/.lattice/web-archive
/// - `EmbeddingServiceTrait`: Generates vector embeddings for text chunks
/// - `IndexStorageTrait`: Stores documents with chunks and embeddings
///
/// # Example
///
/// ```rust,no_run
/// use lattice::infrastructure::services::{
///     WebIngestionService, ArticleExtractorService, embedding::OnnxEmbeddingService
/// };
/// use lattice::features::indexing::engine::storage::IndexStorage;
/// use std::sync::Arc;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let article_extractor = Arc::new(ArticleExtractorService::new(/* ... */));
/// let web_archive = Arc::new(WebArchiveService::new()?);
/// let embedding_service = Arc::new(OnnxEmbeddingService::new(/* ... */).await?);
/// let index_storage = Arc::new(IndexStorage::new(/* ... */));
/// let tokenizer = Arc::new(Tokenizer::from_pretrained("bert-base-uncased", None)?);
///
/// let service = WebIngestionService::new(
///     article_extractor,
///     web_archive,
///     embedding_service,
///     index_storage,
///     tokenizer,
/// );
///
/// let result = service.ingest_url("https://example.com/article").await?;
/// println!("Ingested: {} ({})", result.title, result.document_id);
/// # Ok(())
/// # }
/// ```
pub struct WebIngestionService {
    pub(super) article_extractor: Arc<dyn ArticleExtractorServiceTrait>,
    pub(super) web_archive: Arc<dyn WebArchiveServiceTrait>,
    pub(super) embedding_service: Arc<dyn EmbeddingServiceTrait>,
    pub(super) index_storage: Arc<dyn IndexStorageTrait>,
    // reason: required dependency of the public constructors and of the builder's
    // missing-dependency check; chunking currently goes through the embedding service.
    #[allow(dead_code)]
    pub(super) tokenizer: Arc<Tokenizer>,
    pub(super) config: WebIngestionConfig,
}

impl WebIngestionService {
    /// Create a new web ingestion service
    ///
    /// # Arguments
    ///
    /// * `article_extractor` - Service for extracting article content
    /// * `web_archive` - Service for archiving articles as markdown files
    /// * `embedding_service` - Service for generating embeddings
    /// * `index_storage` - Storage for documents and embeddings
    /// * `tokenizer` - Tokenizer for text chunking
    pub fn new(
        article_extractor: Arc<dyn ArticleExtractorServiceTrait>,
        web_archive: Arc<dyn WebArchiveServiceTrait>,
        embedding_service: Arc<dyn EmbeddingServiceTrait>,
        index_storage: Arc<dyn IndexStorageTrait>,
        tokenizer: Arc<Tokenizer>,
    ) -> Self {
        Self {
            article_extractor,
            web_archive,
            embedding_service,
            index_storage,
            tokenizer,
            config: WebIngestionConfig::default(),
        }
    }

    /// Create a builder for constructing `WebIngestionService`.
    pub fn builder() -> WebIngestionServiceBuilder {
        WebIngestionServiceBuilder::new()
    }

    /// Create service with explicit runtime configuration.
    pub fn with_config(
        article_extractor: Arc<dyn ArticleExtractorServiceTrait>,
        web_archive: Arc<dyn WebArchiveServiceTrait>,
        embedding_service: Arc<dyn EmbeddingServiceTrait>,
        index_storage: Arc<dyn IndexStorageTrait>,
        tokenizer: Arc<Tokenizer>,
        config: WebIngestionConfig,
    ) -> Result<Self> {
        Self::validate_config(&config)?;

        Ok(Self {
            article_extractor,
            web_archive,
            embedding_service,
            index_storage,
            tokenizer,
            config,
        })
    }
}

#[async_trait]
impl WebIngestionServiceTrait for WebIngestionService {
    async fn ingest_url(&self, url: &str) -> Result<WebIngestionResult> {
        // 1. Extract article/video content from URL using strategy pipeline
        let article = self.extract_content_with_strategies(url).await?;

        // 2. Archive article as markdown file in ~/.lattice/web-archive
        let file_path = self
            .web_archive
            .archive_article(article.clone(), url)
            .await?;

        // 3. Chunk text content (fixed-size chunks)
        let model_identity = self.embedding_service.model_identity();
        let chunks = self.chunk_text(&article.text_content)?;

        // 4. Generate embeddings
        //
        // Span-grouped, like the file importer: with late chunking the article
        // is embedded once and pooled per chunk, and otherwise this is the same
        // per-chunk batch as before. Embedding chunk-first here while files are
        // late-chunked would put two vector spaces in one generation. The
        // stored chunk content and its offsets are identical either way — only
        // which forward pass produced the numbers differs.
        let embeddings = match Self::span_for_chunks(&article.text_content, &chunks) {
            Some(span) if self.embedding_service.uses_late_chunking() => {
                let vectors = self
                    .embedding_service
                    .embed_span_chunks(&span.span_text, &span.chunk_ranges)
                    .await?;
                if vectors.len() != chunks.len() {
                    return Err(AppError::InvalidState(format!(
                        "Late chunking returned {} vectors for {} chunks",
                        vectors.len(),
                        chunks.len()
                    )));
                }
                vectors
            }
            _ => {
                let chunk_texts: Vec<String> = chunks
                    .iter()
                    .map(|c| c.contextualized_content.clone())
                    .collect();
                self.embedding_service.embed_batch(&chunk_texts).await?
            }
        };

        if self.embedding_service.model_identity() != model_identity {
            return Err(AppError::InvalidState(
                "Embedding model changed during web import; retry".into(),
            ));
        }

        // 5. Store document + chunks + embeddings (using the real file path)
        let stored_doc_id = self
            .index_storage
            .store_document_with_context_for_model(
                &file_path,
                "text/markdown",
                chunks.clone(),
                embeddings,
                &model_identity,
            )
            .await?;

        // 6. Return result
        Ok(WebIngestionResult {
            document_id: stored_doc_id,
            url: url.to_string(),
            title: article.title.clone(),
            word_count: article.word_count,
            chunks_created: chunks.len(),
            site_name: None, // Not extracted by article extractor
            author: article.author,
            reading_time_minutes: Some(article.reading_time_minutes),
        })
    }
}
