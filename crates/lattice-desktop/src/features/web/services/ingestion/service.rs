//! The `WebIngestionService` type and its end-to-end ingestion workflow.

use super::{WebIngestionConfig, WebIngestionServiceBuilder};
use crate::features::web::traits::ArticleExtractorServiceTrait;
use crate::features::web::{WebArchiveServiceTrait, WebIngestionResult, WebIngestionServiceTrait};
use async_trait::async_trait;
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::application::ports::IngestedDocumentStorePort;
use lattice_core::application::ports::{SearchResultCachePort, VectorSearchPort};
use lattice_core::application::services::live_index::{
    publish_chunks_to_live_index, PublishedChunk,
};
use lattice_core::shared::error::{AppError, Result};
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
pub struct WebIngestionService {
    pub(super) article_extractor: Arc<dyn ArticleExtractorServiceTrait>,
    pub(super) web_archive: Arc<dyn WebArchiveServiceTrait>,
    pub(super) embedding_service: Arc<dyn EmbeddingPort>,
    pub(super) index_storage: Arc<dyn IngestedDocumentStorePort>,
    // reason: required dependency of the public constructors and of the builder's
    // missing-dependency check; chunking currently goes through the embedding service.
    #[allow(dead_code)]
    pub(super) tokenizer: Arc<Tokenizer>,
    pub(super) config: WebIngestionConfig,
    /// Live vector index; see [`WebIngestionServiceBuilder::vector_search`].
    pub(super) vector_search: Option<Arc<dyn VectorSearchPort>>,
    /// Search cache; see [`WebIngestionServiceBuilder::query_cache`].
    pub(super) query_cache: Option<Arc<dyn SearchResultCachePort>>,
}

impl WebIngestionService {
    /// Create a new web ingestion service
    pub fn new(
        article_extractor: Arc<dyn ArticleExtractorServiceTrait>,
        web_archive: Arc<dyn WebArchiveServiceTrait>,
        embedding_service: Arc<dyn EmbeddingPort>,
        index_storage: Arc<dyn IngestedDocumentStorePort>,
        tokenizer: Arc<Tokenizer>,
    ) -> Self {
        Self {
            article_extractor,
            web_archive,
            embedding_service,
            index_storage,
            tokenizer,
            config: WebIngestionConfig::default(),
            vector_search: None,
            query_cache: None,
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
        embedding_service: Arc<dyn EmbeddingPort>,
        index_storage: Arc<dyn IngestedDocumentStorePort>,
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
            vector_search: None,
            query_cache: None,
        })
    }

    pub(super) fn with_vector_search(
        mut self,
        vector_search: Option<Arc<dyn VectorSearchPort>>,
    ) -> Self {
        self.vector_search = vector_search;
        self
    }

    pub(super) fn with_query_cache(
        mut self,
        query_cache: Option<Arc<dyn SearchResultCachePort>>,
    ) -> Self {
        self.query_cache = query_cache;
        self
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
        let chunk_texts: Vec<String> = chunks
            .iter()
            .map(|c| c.contextualized_content.clone())
            .collect();
        let embeddings = self.embedding_service.embed_batch(&chunk_texts).await?;

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
                embeddings.clone(),
                &model_identity,
            )
            .await?;

        // 6. Publish to the live vector index, as file imports do. The rows
        // above are only in SQLite; without this the article is missing from
        // dense search until the next launch rebuilds the whole index.
        if let Some(search) = self.vector_search.clone() {
            let chunk_ids = self
                .index_storage
                .chunk_ids_in_order(&stored_doc_id)
                .await?;
            if chunk_ids.len() != chunks.len() {
                return Err(AppError::InvalidState(format!(
                    "Web import stored {} chunks but {} were prepared",
                    chunk_ids.len(),
                    chunks.len()
                )));
            }
            let published = chunk_ids
                .into_iter()
                .zip(chunks.iter())
                .zip(embeddings)
                .map(|((chunk_id, chunk), embedding)| PublishedChunk {
                    chunk_id,
                    content: chunk.original_content.clone(),
                    embedding,
                })
                .collect();
            publish_chunks_to_live_index(
                search,
                self.query_cache.as_deref(),
                &stored_doc_id,
                published,
            )
            .await?;
        }

        // 7. Return result
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
