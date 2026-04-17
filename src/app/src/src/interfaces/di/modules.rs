//! Domain Modules - Modular Replacement for ServiceContainer
//!
//! This module replaces the monolithic ServiceContainer (102 fields) with 7 focused
//! domain modules. Each module contains only the use cases and services relevant to
//! its bounded context.
//!
//! **Approved Architecture** (2026-01-21):
//! - Fixes stack overflow caused by God Object anti-pattern
//! - Aligns with DDD Bounded Contexts
//! - Follows Tauri 2.0 best practices (multiple State objects)
//! - Reduces command context from 102 pointers to ~5-15 per module
//!
//! ## Modules:
//! 1. **CoreModule** - Shared infrastructure (DB, security, runtime)
//! 2. **SearchModule** - All search functionality
//! 3. **IndexingModule** - Document ingestion and processing
//! 4. **AIModule** - LLM, Q&A, conversations
//! 5. **LibraryModule** - Tags, favorites, mentions
//! 6. **FileOpsModule** - File system operations
//! 7. **SystemModule** - Settings, health, backup, cache

use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;

use crate::infrastructure::security::{FileAccessConfig, SecurityContext};

// Application Use Cases - Search
use crate::features::search::use_cases::{
    FileSearchUseCase, HybridSearchUseCase, RecencySearchUseCase, SemanticSearchUseCase,
};

// Application Use Cases - Indexing
use crate::features::indexing::use_cases::{
    DeleteDocumentUseCase, IndexDirectoryUseCase, IndexFileUseCase, ReindexDocumentUseCase,
    RenameDocumentUseCase,
};

// Application Use Cases - Web
use crate::features::web::use_cases::{
    CleanArticleContentUseCase, GetUrlPreviewUseCase, IngestWebUrlUseCase,
};

// Application Use Cases - Batch
use crate::features::batch::use_cases::{
    CancelBatchJobUseCase, DeleteBatchJobUseCase, GetBatchFileStatusUseCase,
    GetBatchJobStatusUseCase, ListBatchJobsUseCase, RetryFailedItemsUseCase,
    StartBatchFileImportUseCase, StartBatchUrlImportUseCase,
};

// Application Use Cases - Conversation
use crate::features::conversation::use_cases::{
    CreateConversationUseCase, DeleteConversationUseCase, GetConversationMessagesUseCase,
    GetConversationUseCase, ListConversationsUseCase, RenameConversationUseCase,
};

// Application Use Cases - LLM
use crate::features::llm::use_cases::{
    CheckModelDownloadedUseCase, DeleteModelUseCase, DownloadModelUseCase,
    GetAvailableModelsUseCase, GetBestModelUseCase, GetModelPathUseCase,
    GetRecommendedModelsUseCase, GetSystemCapabilitiesUseCase, ListDownloadedModelsUseCase,
};

// Application Use Cases - Tags
use crate::features::tags::use_cases::{
    ApplyTagsUseCase, AutoTagAllDocumentsUseCase, CreateTagUseCase, DeleteTagUseCase,
    GenerateTagsUseCase, GetTagsUseCase, RemoveTagFromDocumentUseCase, SearchByTagUseCase,
    UpdateTagUseCase,
};

// Application Use Cases - Favorites
use crate::features::favorites::use_cases::{
    AddFavoriteUseCase, IsFavoriteUseCase, ListFavoritesUseCase, RemoveFavoriteUseCase,
};

// Application Use Cases - Recent
use crate::features::recent::use_cases::{
    ClearRecentHistoryUseCase, GetRecentDocumentsUseCase, TrackAccessUseCase,
};

// Application Use Cases - Mentions
use crate::features::mentions::use_cases::{
    CreateMentionUseCase, DeleteMentionUseCase, ExtractMentionsUseCase, GetBacklinksUseCase,
    GetMentionsByTypeUseCase, GetMentionsForDocumentUseCase, SearchMentionsUseCase,
    UpdateMentionUseCase,
};

// Application Use Cases - File Operations
use crate::features::file::use_cases::{
    GetFileMetadataUseCase, GetFilePathByIdUseCase, OpenFileByIdUseCase, OpenFileUseCase,
    ReadFileBytesUseCase, ReadFileContentUseCase, ShowInFolderUseCase, UpdateFileMetadataUseCase,
};

// Application Use Cases - Extraction
use crate::features::extraction::use_cases::extract_and_resolve_links::{
    ParseWikilinksPort, ResolveWikilinkPort,
};
use crate::features::extraction::use_cases::{
    ExtractAndResolveLinksUseCase, ExtractDocumentTitleUseCase, ParseWikilinksUseCase,
    ResolveWikilinkUseCase,
};

// Application Use Cases - Health
use crate::features::health::use_cases::HealthCheckUseCase;
use crate::features::initialization::use_cases::{
    InitializeDatabaseUseCase, InitializeModelsUseCase,
};

// Application Use Cases - Settings
use crate::features::settings::use_cases::{
    ExportSettingsUseCase, GetSettingsUseCase, ImportSettingsUseCase, ResetSettingsUseCase,
    UpdateSettingsUseCase, ValidateSettingsUseCase,
};

// Application Use Cases - Cache
use crate::features::cache::use_cases::{
    ClearCacheUseCase, GetCacheSizeUseCase, GetCacheStatsUseCase,
};

// Application Use Cases - Backup
use crate::features::backup::use_cases::{
    CreateBackupUseCase, ListBackupsUseCase, RestoreBackupUseCase, StartAutoBackupUseCase,
    StartupAutoBackupUseCase, StopAutoBackupUseCase,
};

// Application Use Cases - Updates
use crate::features::updates::use_cases::{CheckForUpdatesUseCase, GetCurrentVersionUseCase};

// Application Use Cases - Metrics
use crate::features::metrics::use_cases::GetMetricsUseCase;

// Application Use Cases - Stats
use crate::features::stats::use_cases::GetSystemStatsUseCase;
use crate::domain::embedding_constants::{DEFAULT_EMBEDDING_DIM, DEFAULT_EMBEDDING_MODEL_NAME};
use crate::infrastructure::observability::metrics::Metrics;

// Application Use Cases - Credentials
use crate::features::credentials::use_cases::{
    DeleteApiKeyUseCase, GetApiKeyUseCase, SetApiKeyUseCase, SetCustomEndpointUseCase,
};

// Application Ports
use crate::application::ports::BackupSchedulerPort;
use crate::application::ports::{
    BackupPort, BatchJobRepositoryPort, CachePort, ChunkRepositoryPort,
    ContentAddressedStoragePort, ContentExtractionPort, CredentialsPort, DocumentRepository,
    EmbeddingPort, EmbeddingRepositoryPort, FavoritesRepositoryPort, FileStoragePort,
    FileSystemPort, MentionRepositoryPort, MetricsPort, ModelCatalogPort, ModelStoragePort,
    RecentDocumentsRepositoryPort, RepositoryPort, SettingsRepositoryPort, SystemInfoPort,
    TextSearchPort, UpdateCheckerPort, VectorSearchPort,
};

// Service Traits
use crate::infrastructure::services::traits::{
    ArticleExtractorServiceTrait, BM25SearchTrait, BatchFileImportServiceTrait,
    BatchUrlImportServiceTrait, ConversationServiceTrait, ConversationalQAServiceTrait,
    EmbeddingServiceTrait, HybridSearchTrait, IndexStorageTrait, IndexingServiceTrait,
    ModelManagerTrait, SearchEnrichmentServiceTrait, SearchServiceTrait, TagServiceTrait,
    WebArchiveServiceTrait, WebCaptureServiceTrait, WebIngestionServiceTrait,
};

use crate::application::ports::LLMPort;
use crate::infrastructure::llm::inference::InferenceEngine;
use crate::infrastructure::services::model_manager::ModelManager;

// ==============================================================================
// 1. CoreModule - Shared Infrastructure
// ==============================================================================

/// Core shared infrastructure used by all modules
///
/// **Fields**: ~10
/// - Database connection pool
/// - Security context and file access config
/// - Application data directory
/// - Credentials port for API keys
#[derive(Clone)]
pub struct CoreModule {
    // Database
    db_pool: SqlitePool,
    db_conn: Arc<crate::infrastructure::persistence::database::DatabaseConnection>,

    // Security
    security_context: Arc<SecurityContext>,
    file_access_config: Arc<FileAccessConfig>,

    // Paths
    data_dir: PathBuf,

    // Credentials (used by AI, System modules)
    credentials: Arc<dyn CredentialsPort>,

    // Use Cases
    set_api_key_use_case: Arc<SetApiKeyUseCase>,
    get_api_key_use_case: Arc<GetApiKeyUseCase>,
    delete_api_key_use_case: Arc<DeleteApiKeyUseCase>,
    set_custom_endpoint_use_case: Arc<SetCustomEndpointUseCase>,
}

impl CoreModule {
    /// Build Core infrastructure - Layer 1 (Hierarchical init)
    ///
    /// This builds shared singletons used by all feature modules:
    /// - Security context (rate limiting, input validation)
    /// - File access config (CWE-22 mitigation)
    /// - Credentials adapter (secure storage)
    ///
    /// Stack frame is freed after this returns, before feature modules start.
    pub async fn new(
        db_pool: SqlitePool,
        db_conn: Arc<crate::infrastructure::persistence::database::DatabaseConnection>,
        data_dir: PathBuf,
    ) -> crate::shared::error::Result<Self> {
        // === Build Core Infrastructure (Layer 1) ===

        // Security Context (rate limiting + input validation)
        let security_context = Arc::new(SecurityContext::new());

        // File Access Config (CWE-22 mitigation)
        // Default allowed roots: user home directory
        let allowed_roots = vec![dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"))];
        let file_access_config = Arc::new(FileAccessConfig::new(allowed_roots));

        // Credentials Adapter (secure storage)
        use crate::features::credentials::adapter::CredentialsAdapter;
        let credentials_path = data_dir.join("credentials.json");
        let credentials =
            Arc::new(CredentialsAdapter::new(credentials_path)) as Arc<dyn CredentialsPort>;

        // === Build Use Cases (Credentials Management) ===
        let set_api_key_use_case = Arc::new(SetApiKeyUseCase::new(credentials.clone()));
        let get_api_key_use_case = Arc::new(GetApiKeyUseCase::new(credentials.clone()));
        let delete_api_key_use_case = Arc::new(DeleteApiKeyUseCase::new(credentials.clone()));
        let set_custom_endpoint_use_case =
            Arc::new(SetCustomEndpointUseCase::new(credentials.clone()));

        Ok(Self {
            db_pool,
            db_conn,
            security_context,
            file_access_config,
            data_dir,
            credentials,
            set_api_key_use_case,
            get_api_key_use_case,
            delete_api_key_use_case,
            set_custom_endpoint_use_case,
        })
    }

    // Getters
    pub fn db_pool(&self) -> &SqlitePool {
        &self.db_pool
    }

    pub fn db_conn(
        &self,
    ) -> &Arc<crate::infrastructure::persistence::database::DatabaseConnection> {
        &self.db_conn
    }

    pub fn security_context(&self) -> &Arc<SecurityContext> {
        &self.security_context
    }

    pub fn file_access_config(&self) -> &Arc<FileAccessConfig> {
        &self.file_access_config
    }

    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

    pub fn credentials(&self) -> &Arc<dyn CredentialsPort> {
        &self.credentials
    }

    // Use case getters
    pub fn set_api_key_use_case(&self) -> &Arc<SetApiKeyUseCase> {
        &self.set_api_key_use_case
    }

    pub fn get_api_key_use_case(&self) -> &Arc<GetApiKeyUseCase> {
        &self.get_api_key_use_case
    }

    pub fn delete_api_key_use_case(&self) -> &Arc<DeleteApiKeyUseCase> {
        &self.delete_api_key_use_case
    }

    pub fn set_custom_endpoint_use_case(&self) -> &Arc<SetCustomEndpointUseCase> {
        &self.set_custom_endpoint_use_case
    }
}

// ==============================================================================
// 2. SearchModule - All Search Functionality
// ==============================================================================

/// Search module for semantic, hybrid, and file search
///
/// **Fields**: ~15
/// - 4 search use cases
/// - 3 search services
/// - Vector and text search ports
/// - Embedding cache (read-only)
/// - Document and recent docs repositories
#[derive(Clone)]
pub struct SearchModule {
    // Use Cases
    semantic_search_use_case: Arc<SemanticSearchUseCase>,
    hybrid_search_use_case: Arc<HybridSearchUseCase>,
    file_search_use_case: Arc<FileSearchUseCase>,
    recency_search_use_case: Arc<RecencySearchUseCase>,

    // Services
    search_service: Arc<dyn SearchServiceTrait>,
    bm25_search: Arc<dyn BM25SearchTrait>,
    hybrid_search_service: Arc<dyn HybridSearchTrait>,
    search_enrichment_service: Arc<dyn SearchEnrichmentServiceTrait>,

    // Ports
    vector_search: Arc<dyn VectorSearchPort>,
    text_search: Arc<dyn TextSearchPort>,
    embedding_cache: Arc<RwLock<Option<Arc<dyn EmbeddingPort>>>>,

    // Repositories
    document_repo: Arc<dyn DocumentRepository>,
    recent_docs_repo: Arc<dyn RecentDocumentsRepositoryPort>,
}

impl SearchModule {
    /// Build SearchModule with its own dependencies - Layer 2
    ///
    /// Constructs all search-related infrastructure internally:
    /// - Vector search index (USearchVectorIndex — unified HNSW + persistence)
    /// - Text search (SQLite FTS)
    /// - Search services (USearch, HybridSearch, BM25, Enrichment)
    /// - Search use cases
    ///
    /// Stack frame is freed after return, independent of other modules.
    pub async fn new(
        db_pool: SqlitePool,
        core: Arc<CoreModule>,
        embedding_cache: Arc<RwLock<Option<Arc<dyn EmbeddingPort>>>>,
    ) -> crate::shared::error::Result<Self> {
        // === Build Search Infrastructure ===

        // USearch Vector Index (single unified index replaces both FlatVectorIndex and HNSW)
        use crate::infrastructure::search::vector_search::USearchVectorIndex;
        let usearch_index_path = core.data_dir().join("usearch_index.usearch");

        let usearch_index = Arc::new(
            USearchVectorIndex::open_or_create(DEFAULT_EMBEDDING_DIM, usearch_index_path.clone())
                .map_err(|e| {
                crate::shared::error::AppError::InternalError(format!(
                    "Failed to initialize USearch index: {}",
                    e
                ))
            })?,
        );

        // If USearch index is empty but embeddings exist in SQLite, rebuild from DB.
        // This handles first-time migration from the old BLOB-based storage.
        if usearch_index.count() == 0 {
            let enriched_query = sqlx::query_as::<_, (String, Vec<u8>, String, String, String)>(
                "SELECT te.id, te.embedding, COALESCE(tc.content, ''), tc.id, COALESCE(tc.document_id, '') \
                 FROM text_embeddings te \
                 LEFT JOIN text_chunks tc ON te.chunk_id = tc.id"
            );
            let enriched_rows = enriched_query.fetch_all(&db_pool).await?;

            if !enriched_rows.is_empty() {
                let enriched: Vec<(String, Vec<f32>, String, String, String)> = enriched_rows
                    .into_iter()
                    .map(|(emb_id, bytes, content, chunk_id, doc_id)| {
                        let floats = crate::shared::utils::alignment::bytes_to_f32_vec(&bytes)
                            .unwrap_or_else(|_| vec![]);
                        (emb_id, floats, content, chunk_id, doc_id)
                    })
                    .filter(|(_emb_id, v, _, _, _)| v.len() == DEFAULT_EMBEDDING_DIM)
                    .collect();

                if !enriched.is_empty() {
                    tracing::info!(
                        count = enriched.len(),
                        "Rebuilding USearch index from SQLite embeddings (one-time migration)"
                    );
                    match usearch_index.rebuild_from_embeddings(enriched) {
                        Ok(added) => {
                            tracing::info!(added, "USearch index rebuilt successfully");
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to rebuild USearch index from SQLite");
                        }
                    }
                }
            }
        }

        tracing::info!(
            size = usearch_index.count(),
            path = %usearch_index_path.display(),
            "USearch vector index ready"
        );

        // USearch implements both VectorSearchPort and SearchServiceTrait
        let vector_search = usearch_index.clone() as Arc<dyn VectorSearchPort>;
        let search_service = usearch_index.clone() as Arc<dyn SearchServiceTrait>;

        // Text Search (SQLite FTS5)
        use crate::infrastructure::search::text_search::SqliteTextSearch;
        let text_search =
            Arc::new(SqliteTextSearch::new(db_pool.clone())) as Arc<dyn TextSearchPort>;

        // Document Repository (for search results)
        use crate::infrastructure::persistence::repositories::DocumentRepositoryImpl;
        let document_repo =
            Arc::new(DocumentRepositoryImpl::new(db_pool.clone())) as Arc<dyn DocumentRepository>;

        // Recent Documents Repository (for recency search)
        use crate::features::recent::repository::RecentDocumentsRepository;
        let recent_docs_repo = Arc::new(RecentDocumentsRepository::new(db_pool.clone()))
            as Arc<dyn RecentDocumentsRepositoryPort>;

        // BM25 Search (keyword-based)
        use crate::infrastructure::search::bm25::BM25Search;
        let bm25_search = Arc::new(BM25Search::new(db_pool.clone())) as Arc<dyn BM25SearchTrait>;

        // Search Enrichment Service (result enhancement)
        use crate::infrastructure::services::search_enrichment_service::SearchEnrichmentService;
        let search_enrichment_service = Arc::new(SearchEnrichmentService::new(db_pool.clone()))
            as Arc<dyn SearchEnrichmentServiceTrait>;

        // Hybrid Search Service (combines vector + keyword)
        use crate::infrastructure::search::hybrid::HybridSearchService;
        use crate::infrastructure::search::hybrid::{SearchConfig, SearchMode};
        let hybrid_config = SearchConfig {
            mode: SearchMode::Hybrid,
            vector_weight: 0.7,
            keyword_weight: 0.3,
            min_score: crate::shared::constants::MIN_SIMILARITY_SCORE,
            enable_reranking: true,
            recency_boost: 1.0,
            max_results: 100,
        };
        let hybrid_search_service = Arc::new(HybridSearchService::new(
            search_service.clone(),
            bm25_search.clone(),
            db_pool.clone(),
            search_enrichment_service.clone(),
            hybrid_config,
        )) as Arc<dyn HybridSearchTrait>;

        // === Build Dynamic Embedding Adapter ===
        // Wrapper that checks embedding_cache and falls back to mock if None
        struct DynamicEmbedding {
            cache: Arc<RwLock<Option<Arc<dyn EmbeddingPort>>>>,
        }

        #[async_trait::async_trait]
        impl EmbeddingPort for DynamicEmbedding {
            async fn embed_single(&self, text: &str) -> crate::shared::result::Result<Vec<f32>> {
                // Clone Arc before dropping lock to avoid holding guard across await
                let embedding_opt = {
                    let cache_read = self.cache.read().map_err(|e| {
                        crate::shared::error::AppError::InternalError(format!(
                            "Embedding cache lock poisoned: {}",
                            e
                        ))
                    })?;
                    cache_read.as_ref().cloned()
                }; // Lock guard dropped here

                if let Some(embedding) = embedding_opt {
                    // Use real embedding if loaded
                    embedding.embed_single(text).await
                } else {
                    // Fall back to mock (degraded mode)
                    use crate::application::ports::MockEmbeddingPort;
                    MockEmbeddingPort::new_degraded().embed_single(text).await
                }
            }

            async fn embed_batch(
                &self,
                texts: &[String],
            ) -> crate::shared::result::Result<Vec<Vec<f32>>> {
                // Clone Arc before dropping lock to avoid holding guard across await
                let embedding_opt = {
                    let cache_read = self.cache.read().map_err(|e| {
                        crate::shared::error::AppError::InternalError(format!(
                            "Embedding cache lock poisoned: {}",
                            e
                        ))
                    })?;
                    cache_read.as_ref().cloned()
                }; // Lock guard dropped here

                if let Some(embedding) = embedding_opt {
                    // Use real embedding if loaded
                    embedding.embed_batch(texts).await
                } else {
                    // Fall back to mock (degraded mode)
                    use crate::application::ports::MockEmbeddingPort;
                    MockEmbeddingPort::new_degraded().embed_batch(texts).await
                }
            }

            fn dimension(&self) -> usize {
                // Return standard dimension (DEFAULT_EMBEDDING_DIM)
                // This matches MockEmbeddingPort::dimension() for consistency
                DEFAULT_EMBEDDING_DIM
            }

            async fn is_ready(&self) -> crate::shared::result::Result<bool> {
                // Clone Arc before dropping lock to avoid holding guard across await
                let embedding_opt = {
                    let cache_read = self.cache.read().map_err(|e| {
                        crate::shared::error::AppError::InternalError(format!(
                            "Embedding cache lock poisoned: {}",
                            e
                        ))
                    })?;
                    cache_read.as_ref().cloned()
                }; // Lock guard dropped here

                if let Some(embedding) = embedding_opt {
                    // Check if real embedding is ready
                    embedding.is_ready().await
                } else {
                    // Mock is never ready
                    Ok(false)
                }
            }
        }

        let dynamic_embedding = Arc::new(DynamicEmbedding {
            cache: embedding_cache.clone(),
        }) as Arc<dyn EmbeddingPort>;

        // === Build Use Cases ===

        let semantic_search_use_case = Arc::new(SemanticSearchUseCase::new(
            dynamic_embedding.clone(),
            vector_search.clone(),
        ));

        let hybrid_search_use_case = Arc::new(HybridSearchUseCase::new(
            dynamic_embedding.clone(),
            vector_search.clone(),
            text_search.clone(),
        ));

        let file_search_use_case = Arc::new(FileSearchUseCase::new(text_search.clone()));

        let recency_search_use_case = Arc::new(RecencySearchUseCase::new(
            dynamic_embedding.clone(),
            hybrid_search_service.clone(),
        ));

        Ok(Self {
            semantic_search_use_case,
            hybrid_search_use_case,
            file_search_use_case,
            recency_search_use_case,
            search_service,
            bm25_search,
            hybrid_search_service,
            search_enrichment_service,
            vector_search,
            text_search,
            embedding_cache,
            document_repo,
            recent_docs_repo,
        })
    }

    // Getters for use cases
    pub fn semantic_search_use_case(&self) -> &Arc<SemanticSearchUseCase> {
        &self.semantic_search_use_case
    }

    pub fn hybrid_search_use_case(&self) -> &Arc<HybridSearchUseCase> {
        &self.hybrid_search_use_case
    }

    pub fn file_search_use_case(&self) -> &Arc<FileSearchUseCase> {
        &self.file_search_use_case
    }

    pub fn recency_search_use_case(&self) -> &Arc<RecencySearchUseCase> {
        &self.recency_search_use_case
    }

    // Getters for services (backward compatibility with old commands)
    pub fn search_service(&self) -> &Arc<dyn SearchServiceTrait> {
        &self.search_service
    }

    pub fn bm25_search_service(&self) -> &Arc<dyn BM25SearchTrait> {
        &self.bm25_search
    }

    pub fn hybrid_search_service(&self) -> &Arc<dyn HybridSearchTrait> {
        &self.hybrid_search_service
    }

    pub fn search_enrichment_service(&self) -> &Arc<dyn SearchEnrichmentServiceTrait> {
        &self.search_enrichment_service
    }

    // Port getters (for Container - needed for Q&A use case and embedding cache)
    pub fn embedding_cache(&self) -> &Arc<RwLock<Option<Arc<dyn EmbeddingPort>>>> {
        &self.embedding_cache
    }

    pub fn vector_search(&self) -> &Arc<dyn VectorSearchPort> {
        &self.vector_search
    }

    pub fn document_repo(&self) -> &Arc<dyn DocumentRepository> {
        &self.document_repo
    }
}

// ==============================================================================
// 3. IndexingModule - Document Ingestion and Processing
// ==============================================================================

/// Indexing module for document ingestion, web scraping, and batch imports
///
/// **Fields**: ~20
/// - 16 use cases (indexing, web, batch)
/// - 7 services
/// - Embedding cache (write access)
/// - File storage port
/// - Document and batch job repositories
#[derive(Clone)]
pub struct IndexingModule {
    // Use Cases - Indexing
    index_file_use_case: Arc<IndexFileUseCase>,
    index_directory_use_case: Arc<IndexDirectoryUseCase>,
    reindex_document_use_case: Arc<ReindexDocumentUseCase>,
    delete_document_use_case: Arc<DeleteDocumentUseCase>,
    rename_document_use_case: Arc<RenameDocumentUseCase>,

    // Use Cases - Web
    ingest_web_url_use_case: Arc<IngestWebUrlUseCase>,
    get_url_preview_use_case: Arc<GetUrlPreviewUseCase>,
    clean_article_content_use_case: Arc<CleanArticleContentUseCase>,

    // Use Cases - Batch
    start_batch_file_import_use_case: Arc<StartBatchFileImportUseCase>,
    get_batch_file_status_use_case: Arc<GetBatchFileStatusUseCase>,
    start_batch_url_import_use_case: Arc<StartBatchUrlImportUseCase>,
    get_batch_job_status_use_case: Arc<GetBatchJobStatusUseCase>,
    cancel_batch_job_use_case: Arc<CancelBatchJobUseCase>,
    list_batch_jobs_use_case: Arc<ListBatchJobsUseCase>,
    delete_batch_job_use_case: Arc<DeleteBatchJobUseCase>,
    retry_failed_items_use_case: Arc<RetryFailedItemsUseCase>,

    // Services
    indexing_service: Arc<dyn IndexingServiceTrait>,
    indexing_state: Arc<crate::infrastructure::indexing::IndexingState>,
    web_ingestion_service: Arc<dyn WebIngestionServiceTrait>,
    web_capture_service: Arc<dyn WebCaptureServiceTrait>,
    article_extractor_service: Arc<dyn ArticleExtractorServiceTrait>,
    web_archive: Arc<dyn WebArchiveServiceTrait>,
    batch_file_import_service: Arc<dyn BatchFileImportServiceTrait>,
    batch_url_import_service: Arc<dyn BatchUrlImportServiceTrait>,

    // Ports
    embedding_cache: Arc<RwLock<Option<Arc<dyn EmbeddingPort>>>>,
    file_storage: Arc<dyn FileStoragePort>,

    // Repositories
    document_repo: Arc<dyn DocumentRepository>,
    chunk_repo: Arc<dyn ChunkRepositoryPort>,
    batch_job_repo: Arc<dyn BatchJobRepositoryPort>,

    // File system
    file_system: Arc<dyn FileSystemPort>,
}

impl IndexingModule {
    /// Build IndexingModule with all its dependencies
    ///
    /// Constructs repositories, services, adapters, and use cases for indexing:
    /// - Document indexing (5 operations)
    /// - Web ingestion (3 operations)
    /// - Batch processing (8 operations)
    ///
    /// Note: embedding_cache and vector_search are passed in (shared state with SearchModule)
    pub async fn new(
        db_pool: SqlitePool,
        core: Arc<CoreModule>,
        embedding_cache: Arc<RwLock<Option<Arc<dyn EmbeddingPort>>>>,
        vector_search: Arc<dyn VectorSearchPort>,
    ) -> crate::shared::error::Result<Self> {
        // === Build Repositories ===

        // Document Repository
        use crate::infrastructure::persistence::repositories::DocumentRepositoryImpl;
        let document_repo =
            Arc::new(DocumentRepositoryImpl::new(db_pool.clone())) as Arc<dyn DocumentRepository>;

        // Batch Job Repository
        use crate::infrastructure::persistence::repositories::BatchJobRepository;
        let batch_job_repo =
            Arc::new(BatchJobRepository::new(db_pool.clone())) as Arc<dyn BatchJobRepositoryPort>;

        // Chunk Repository (for delete operations)
        use crate::application::ports::ChunkRepositoryPort;
        use crate::infrastructure::persistence::repositories::ChunkRepositoryImpl;
        let chunk_repo =
            Arc::new(ChunkRepositoryImpl::new(db_pool.clone())) as Arc<dyn ChunkRepositoryPort>;

        // Embedding Repository (for embeddings persistence)
        use crate::infrastructure::persistence::repositories::EmbeddingRepository;
        let embedding_repo =
            Arc::new(EmbeddingRepository::new(db_pool.clone())) as Arc<dyn EmbeddingRepositoryPort>;

        // Unit of Work Factory (for batch transactions)
        use crate::infrastructure::persistence::repositories::unit_of_work::SqliteUnitOfWorkFactory;
        let uow_factory = Arc::new(SqliteUnitOfWorkFactory::new(db_pool.clone()))
            as Arc<dyn crate::domain::repositories::UnitOfWorkFactory>;

        // === Build Adapters ===

        // Content-Addressed Storage (for file imports)
        use crate::infrastructure::storage::content_addressed_storage::ContentAddressedStorage;
        let content_storage =
            Arc::new(ContentAddressedStorage::new()?) as Arc<dyn ContentAddressedStoragePort>;

        // File Storage (secure file operations)
        use crate::infrastructure::file_system::SecureFileStorage;
        let file_storage = Arc::new(SecureFileStorage::new()) as Arc<dyn FileStoragePort>;

        // Content Extractor (text extraction from files)
        use crate::infrastructure::adapters::content_extraction_adapter::ContentExtractionAdapter;
        let content_extractor =
            Arc::new(ContentExtractionAdapter::new()) as Arc<dyn ContentExtractionPort>;

        // File System (from CoreModule)
        use crate::infrastructure::file_system::FileSystemAdapter;
        let file_system = Arc::new(FileSystemAdapter::new()) as Arc<dyn FileSystemPort>;

        // Indexing Embedding adapter - shares embedding cache with SearchModule.
        // If a real embedding model is loaded into cache, indexing uses it.
        // Otherwise, degraded mock behavior is preserved.
        struct DynamicIndexingEmbedding {
            cache: Arc<RwLock<Option<Arc<dyn EmbeddingPort>>>>,
        }

        #[async_trait::async_trait]
        impl EmbeddingPort for DynamicIndexingEmbedding {
            async fn embed_single(&self, text: &str) -> crate::shared::result::Result<Vec<f32>> {
                let embedding_opt = {
                    let cache_read = self.cache.read().map_err(|e| {
                        crate::shared::error::AppError::InternalError(format!(
                            "Embedding cache lock poisoned: {}",
                            e
                        ))
                    })?;
                    cache_read.as_ref().cloned()
                };

                if let Some(embedding) = embedding_opt {
                    embedding.embed_single(text).await
                } else {
                    use crate::application::ports::MockEmbeddingPort;
                    MockEmbeddingPort::new_degraded().embed_single(text).await
                }
            }

            async fn embed_batch(
                &self,
                texts: &[String],
            ) -> crate::shared::result::Result<Vec<Vec<f32>>> {
                let embedding_opt = {
                    let cache_read = self.cache.read().map_err(|e| {
                        crate::shared::error::AppError::InternalError(format!(
                            "Embedding cache lock poisoned: {}",
                            e
                        ))
                    })?;
                    cache_read.as_ref().cloned()
                };

                if let Some(embedding) = embedding_opt {
                    embedding.embed_batch(texts).await
                } else {
                    use crate::application::ports::MockEmbeddingPort;
                    MockEmbeddingPort::new_degraded().embed_batch(texts).await
                }
            }

            fn dimension(&self) -> usize {
                DEFAULT_EMBEDDING_DIM
            }

            async fn is_ready(&self) -> crate::shared::result::Result<bool> {
                let embedding_opt = {
                    let cache_read = self.cache.read().map_err(|e| {
                        crate::shared::error::AppError::InternalError(format!(
                            "Embedding cache lock poisoned: {}",
                            e
                        ))
                    })?;
                    cache_read.as_ref().cloned()
                };

                if let Some(embedding) = embedding_opt {
                    embedding.is_ready().await
                } else {
                    Ok(false)
                }
            }
        }

        let indexing_embedding = Arc::new(DynamicIndexingEmbedding {
            cache: embedding_cache.clone(),
        }) as Arc<dyn EmbeddingPort>;

        // Vector Search: shared with SearchModule (same instance) so that
        // delete operations and runtime indexing operate on the same index
        // that SemanticSearchUseCase queries against.

        // === Build Services ===

        // NOTE: Using degraded implementations for services that require AI models
        // Real implementations are initialized later when models are downloaded
        use crate::infrastructure::setup::degraded_mocks;

        // Indexing Service (degraded - requires embeddings/tokenizer)
        let indexing_service = degraded_mocks::create_degraded_indexing();

        // Web Capture Service (HTTP client - no AI needed)
        use crate::infrastructure::services::WebCaptureService;
        let web_capture_service =
            Arc::new(WebCaptureService::new()?) as Arc<dyn WebCaptureServiceTrait>;

        // Article Extractor Service (HTTP client - no AI needed)
        use crate::infrastructure::services::ArticleExtractorService;
        let article_extractor_service =
            Arc::new(ArticleExtractorService::new()?) as Arc<dyn ArticleExtractorServiceTrait>;

        // Web Archive Service (file storage - no AI needed)
        use crate::infrastructure::services::WebArchiveService;
        let web_archive = Arc::new(WebArchiveService::new().map_err(|e| {
            crate::shared::error::AppError::Other(format!(
                "Failed to create web archive service: {}",
                e
            ))
        })?) as Arc<dyn WebArchiveServiceTrait>;

        // Web Ingestion Service (dynamic - uses embedding cache + fallback tokenizer)
        use crate::infrastructure::indexing::storage::IndexStorage;
        use crate::features::embedding::service::DynamicEmbeddingService;
        use crate::features::web::services::ingestion::WebIngestionService;
        use tokenizers::models::bpe::BPE;
        use tokenizers::pre_tokenizers::whitespace::Whitespace;
        use tokenizers::Tokenizer;

        fn build_fallback_tokenizer() -> Arc<Tokenizer> {
            use std::collections::HashMap;

            // Minimal BPE tokenizer with whitespace pre-tokenizer.
            // Avoids network fetches and provides deterministic tokenization.
            let mut vocab = HashMap::new();
            for c in b'a'..=b'z' {
                vocab.insert(String::from_utf8(vec![c]).unwrap(), c as u32);
            }
            for c in b'A'..=b'Z' {
                vocab.insert(String::from_utf8(vec![c]).unwrap(), (c + 26) as u32);
            }
            for c in b'0'..=b'9' {
                vocab.insert(String::from_utf8(vec![c]).unwrap(), (c + 52) as u32);
            }
            vocab.insert(" ".to_string(), 62);
            vocab.insert(".".to_string(), 63);
            vocab.insert(",".to_string(), 64);
            vocab.insert("-".to_string(), 65);
            vocab.insert("[UNK]".to_string(), 66);

            let merges = vec![];
            let bpe = BPE::builder()
                .vocab_and_merges(vocab, merges)
                .unk_token("[UNK]".to_string())
                .build()
                .expect("Failed to build fallback tokenizer");

            let mut tokenizer = Tokenizer::new(bpe);
            tokenizer.with_pre_tokenizer(Whitespace {});

            Arc::new(tokenizer)
        }

        let model_dir = core.data_dir().join("models");
        let tokenizer = crate::infrastructure::setup::setup_tokenizer(&model_dir)
            .unwrap_or_else(build_fallback_tokenizer);

        let embedding_service = Arc::new(DynamicEmbeddingService::new(embedding_cache.clone()))
            as Arc<dyn EmbeddingServiceTrait>;
        let index_storage =
            Arc::new(IndexStorage::new(db_pool.clone())) as Arc<dyn IndexStorageTrait>;

        let web_ingestion_service = Arc::new(
            WebIngestionService::builder()
                .article_extractor(article_extractor_service.clone())
                .web_archive(web_archive.clone())
                .embedding_service(embedding_service)
                .index_storage(index_storage)
                .tokenizer(tokenizer)
                .build()?,
        ) as Arc<dyn WebIngestionServiceTrait>;

        // Batch File Import Service (degraded - depends on indexing service)
        let batch_file_import_service = degraded_mocks::create_degraded_batch_file_import();

        // Batch URL Import Service (uses web ingestion service)
        use crate::infrastructure::services::BatchUrlImportService;
        let batch_url_import_service = Arc::new(BatchUrlImportService::new(
            web_ingestion_service.clone(),
            batch_job_repo.clone(),
        )) as Arc<dyn BatchUrlImportServiceTrait>;

        // === Build Use Cases ===

        // Indexing state (progress tracking)
        let indexing_state = Arc::new(crate::infrastructure::indexing::IndexingState::new());

        // Indexing use cases
        use crate::application::ports::DocumentRepositoryPort;
        use crate::features::indexing::use_cases::*;
        let index_file_use_case = Arc::new(
            IndexFileUseCase::new(
                content_storage.clone(),
                file_storage.clone(),
                content_extractor.clone(),
                indexing_embedding.clone(),
                document_repo.clone() as Arc<dyn DocumentRepositoryPort>,
                embedding_repo.clone(),
                uow_factory.clone(),
            )
            .with_vector_search(vector_search.clone()),
        );
        let index_directory_use_case = Arc::new(IndexDirectoryUseCase::new(
            index_file_use_case.clone(),
            indexing_state.clone(),
        ));
        let reindex_document_use_case = Arc::new(ReindexDocumentUseCase::new(
            file_storage.clone(),
            indexing_embedding.clone(),
            document_repo.clone() as Arc<dyn DocumentRepositoryPort>,
            uow_factory.clone(),
        ));
        let delete_document_use_case = Arc::new(DeleteDocumentUseCase::new(
            document_repo.clone() as Arc<dyn DocumentRepositoryPort>,
            chunk_repo.clone(),
            vector_search.clone(),
            uow_factory.clone(),
            file_storage.clone(),
        ));
        let rename_document_use_case = Arc::new(RenameDocumentUseCase::new(
            document_repo.clone() as Arc<dyn DocumentRepositoryPort>
        ));

        // Web use cases
        use crate::features::web::use_cases::*;
        let ingest_web_url_use_case =
            Arc::new(IngestWebUrlUseCase::new(web_ingestion_service.clone()));
        let get_url_preview_use_case =
            Arc::new(GetUrlPreviewUseCase::new(web_capture_service.clone()));
        let clean_article_content_use_case = Arc::new(CleanArticleContentUseCase::new(
            article_extractor_service.clone(),
        ));

        // Batch use cases
        use crate::features::batch::use_cases::*;
        let start_batch_file_import_use_case = Arc::new(StartBatchFileImportUseCase::new(
            batch_job_repo.clone(),
            index_file_use_case.clone(),
            uow_factory.clone(),
        ));
        let get_batch_file_status_use_case =
            Arc::new(GetBatchFileStatusUseCase::new(batch_job_repo.clone()));
        let start_batch_url_import_use_case = Arc::new(StartBatchUrlImportUseCase::new(
            batch_job_repo.clone(),
            ingest_web_url_use_case.clone(),
        ));
        let get_batch_job_status_use_case =
            Arc::new(GetBatchJobStatusUseCase::new(batch_job_repo.clone()));
        let cancel_batch_job_use_case =
            Arc::new(CancelBatchJobUseCase::new(batch_job_repo.clone()));
        let list_batch_jobs_use_case = Arc::new(ListBatchJobsUseCase::new(batch_job_repo.clone()));
        let delete_batch_job_use_case =
            Arc::new(DeleteBatchJobUseCase::new(batch_job_repo.clone()));
        // Note: RetryFailedItemsUseCase needs StartBatchUrlImportUseCase but we haven't created it yet
        // We need to create it after start_batch_url_import_use_case
        let retry_failed_items_use_case = Arc::new(RetryFailedItemsUseCase::new(
            batch_job_repo.clone(),
            start_batch_url_import_use_case.clone(),
        ));

        Ok(Self {
            index_file_use_case,
            index_directory_use_case,
            reindex_document_use_case,
            delete_document_use_case,
            rename_document_use_case,
            ingest_web_url_use_case,
            get_url_preview_use_case,
            clean_article_content_use_case,
            start_batch_file_import_use_case,
            get_batch_file_status_use_case,
            start_batch_url_import_use_case,
            get_batch_job_status_use_case,
            cancel_batch_job_use_case,
            list_batch_jobs_use_case,
            delete_batch_job_use_case,
            retry_failed_items_use_case,
            indexing_service,
            indexing_state,
            web_ingestion_service,
            web_capture_service,
            article_extractor_service,
            web_archive,
            batch_file_import_service,
            batch_url_import_service,
            embedding_cache,
            file_storage,
            document_repo,
            chunk_repo,
            batch_job_repo,
            file_system,
        })
    }

    // Use case getters - Indexing
    pub fn index_file_use_case(&self) -> &Arc<IndexFileUseCase> {
        &self.index_file_use_case
    }

    pub fn index_directory_use_case(&self) -> &Arc<IndexDirectoryUseCase> {
        &self.index_directory_use_case
    }

    pub fn reindex_document_use_case(&self) -> &Arc<ReindexDocumentUseCase> {
        &self.reindex_document_use_case
    }

    pub fn delete_document_use_case(&self) -> &Arc<DeleteDocumentUseCase> {
        &self.delete_document_use_case
    }

    pub fn rename_document_use_case(&self) -> &Arc<RenameDocumentUseCase> {
        &self.rename_document_use_case
    }

    // Use case getters - Web
    pub fn ingest_web_url_use_case(&self) -> &Arc<IngestWebUrlUseCase> {
        &self.ingest_web_url_use_case
    }

    pub fn get_url_preview_use_case(&self) -> &Arc<GetUrlPreviewUseCase> {
        &self.get_url_preview_use_case
    }

    pub fn clean_article_content_use_case(&self) -> &Arc<CleanArticleContentUseCase> {
        &self.clean_article_content_use_case
    }

    // Use case getters - Batch
    pub fn start_batch_file_import_use_case(&self) -> &Arc<StartBatchFileImportUseCase> {
        &self.start_batch_file_import_use_case
    }

    pub fn get_batch_file_status_use_case(&self) -> &Arc<GetBatchFileStatusUseCase> {
        &self.get_batch_file_status_use_case
    }

    pub fn start_batch_url_import_use_case(&self) -> &Arc<StartBatchUrlImportUseCase> {
        &self.start_batch_url_import_use_case
    }

    pub fn get_batch_job_status_use_case(&self) -> &Arc<GetBatchJobStatusUseCase> {
        &self.get_batch_job_status_use_case
    }

    pub fn cancel_batch_job_use_case(&self) -> &Arc<CancelBatchJobUseCase> {
        &self.cancel_batch_job_use_case
    }

    pub fn list_batch_jobs_use_case(&self) -> &Arc<ListBatchJobsUseCase> {
        &self.list_batch_jobs_use_case
    }

    pub fn delete_batch_job_use_case(&self) -> &Arc<DeleteBatchJobUseCase> {
        &self.delete_batch_job_use_case
    }

    pub fn retry_failed_items_use_case(&self) -> &Arc<RetryFailedItemsUseCase> {
        &self.retry_failed_items_use_case
    }

    // Service getters (backward compatibility)
    pub fn indexing_state(&self) -> &Arc<crate::infrastructure::indexing::IndexingState> {
        &self.indexing_state
    }

    pub fn indexing_service(&self) -> &Arc<dyn IndexingServiceTrait> {
        &self.indexing_service
    }

    pub fn web_ingestion_service(&self) -> &Arc<dyn WebIngestionServiceTrait> {
        &self.web_ingestion_service
    }

    pub fn web_capture_service(&self) -> &Arc<dyn WebCaptureServiceTrait> {
        &self.web_capture_service
    }

    pub fn article_extractor_service(&self) -> &Arc<dyn ArticleExtractorServiceTrait> {
        &self.article_extractor_service
    }

    pub fn web_archive(&self) -> &Arc<dyn WebArchiveServiceTrait> {
        &self.web_archive
    }

    pub fn batch_file_import_service(&self) -> &Arc<dyn BatchFileImportServiceTrait> {
        &self.batch_file_import_service
    }

    pub fn batch_url_import_service(&self) -> &Arc<dyn BatchUrlImportServiceTrait> {
        &self.batch_url_import_service
    }

    pub fn file_storage(&self) -> &Arc<dyn FileStoragePort> {
        &self.file_storage
    }

    pub fn chunk_repository(&self) -> &Arc<dyn ChunkRepositoryPort> {
        &self.chunk_repo
    }

    pub fn batch_job_repo(&self) -> &Arc<dyn BatchJobRepositoryPort> {
        &self.batch_job_repo
    }
}

// ==============================================================================
// 4. AIModule - LLM, Q&A, Conversations
// ==============================================================================

/// AI module for LLM operations, Q&A, and conversations
///
/// **Fields**: ~25
/// - 17 use cases (conversation, LLM, AI-powered tags)
/// - 4 services
/// - LLM cache and inference engine
/// - Downloaded model repository
/// - Model catalog and credentials
#[derive(Clone)]
pub struct AIModule {
    // Use Cases - Conversation
    create_conversation_use_case: Arc<CreateConversationUseCase>,
    list_conversations_use_case: Arc<ListConversationsUseCase>,
    get_conversation_use_case: Arc<GetConversationUseCase>,
    get_conversation_messages_use_case: Arc<GetConversationMessagesUseCase>,
    rename_conversation_use_case: Arc<RenameConversationUseCase>,
    delete_conversation_use_case: Arc<DeleteConversationUseCase>,

    // Use Cases - LLM
    get_system_capabilities_use_case: Arc<GetSystemCapabilitiesUseCase>,
    get_available_models_use_case: Arc<GetAvailableModelsUseCase>,
    get_recommended_models_use_case: Arc<GetRecommendedModelsUseCase>,
    get_best_model_use_case: Arc<GetBestModelUseCase>,
    check_model_downloaded_use_case: Arc<CheckModelDownloadedUseCase>,
    get_model_path_use_case: Arc<GetModelPathUseCase>,
    download_model_use_case: Arc<DownloadModelUseCase>,
    delete_model_use_case: Arc<DeleteModelUseCase>,
    list_models_use_case: Arc<ListDownloadedModelsUseCase>,

    // Use Cases - AI-Powered Tags
    generate_tags_use_case: Arc<GenerateTagsUseCase>,
    auto_tag_all_documents_use_case: Arc<AutoTagAllDocumentsUseCase>,

    // Services
    conversation_service: Arc<dyn ConversationServiceTrait>,
    conversational_qa_service: Arc<dyn ConversationalQAServiceTrait>,
    llm_cache: Arc<RwLock<Option<Arc<dyn LLMPort>>>>,
    inference_engine_cache: Arc<RwLock<Option<Arc<InferenceEngine>>>>,

    // Repositories
    downloaded_model_repo:
        Arc<crate::infrastructure::persistence::repositories::DownloadedModelRepository>,

    // Adapters
    model_catalog: Arc<dyn ModelCatalogPort>,
    model_catalog_cache: Arc<crate::infrastructure::model_cache_adapter::ModelCacheAdapter>,
    credentials: Arc<dyn CredentialsPort>,

    // Services
    download_manager: Arc<dyn crate::features::download::manager::DownloadManager>,
}

impl AIModule {
    /// Build AIModule with all its dependencies
    ///
    /// Constructs repositories, services, adapters, and use cases for AI features:
    /// - Conversations (create, list, get, rename, delete)
    /// - LLM model management (9 model-related operations)
    /// - AI-powered tags (generate, auto-tag)
    ///
    /// Note: llm_cache and inference_engine_cache are passed in (shared state)
    pub async fn new(
        db_pool: SqlitePool,
        core: Arc<CoreModule>,
        llm_cache: Arc<RwLock<Option<Arc<dyn LLMPort>>>>,
        inference_engine_cache: Arc<RwLock<Option<Arc<InferenceEngine>>>>,
        _llm_endpoint: &str,
        _llm_model: &str,
    ) -> crate::shared::error::Result<Self> {
        // === Build Repositories ===

        // Downloaded Model Repository
        use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
        let downloaded_model_repo = Arc::new(DownloadedModelRepository::new(db_pool.clone()));

        // === Build Adapters ===

        // Model Catalog adapter (Hugging Face + SQLite cache)
        use crate::infrastructure::huggingface_adapter::HuggingFaceAdapter;
        use crate::infrastructure::model_cache_adapter::ModelCacheAdapter;

        let huggingface = Arc::new(HuggingFaceAdapter::new()) as Arc<dyn ModelCatalogPort>;
        let model_catalog_cache =
            Arc::new(ModelCacheAdapter::new(db_pool.clone(), huggingface).await?);
        let model_catalog = model_catalog_cache.clone() as Arc<dyn ModelCatalogPort>;

        // System Info adapter (for system capabilities)
        use crate::infrastructure::system::system_info_adapter::SystemInfoAdapter;
        let system_info = Arc::new(SystemInfoAdapter::new()) as Arc<dyn SystemInfoPort>;

        // Credentials from CoreModule
        let credentials = core.credentials().clone();

        // === Build Services ===

        // Conversation Service
        use crate::infrastructure::services::ConversationService;
        let conversation_service = Arc::new(ConversationService::new(db_pool.clone()))
            as Arc<dyn ConversationServiceTrait>;

        // Conversational QA Service - requires context manager, QA engine, and metrics
        use crate::infrastructure::observability::metrics::Metrics;
        use crate::infrastructure::qa::engine::QAEngine;
        use crate::infrastructure::services::context_manager::ContextManager;
        use crate::features::qa::conversational_service::ConversationalQAService;
        use crate::infrastructure::services::traits::{ContextManagerTrait, QAEngineTrait};

        // Create LLM client from cache (degraded mode if not loaded)
        use crate::infrastructure::llm::noop_client::NoOpLLMClient;
        let llm_client = Arc::new(NoOpLLMClient::new("LLM not loaded yet".into()))
            as Arc<dyn crate::llm::LLMClient>;

        let context_manager = Arc::new(ContextManager::new(8192)) as Arc<dyn ContextManagerTrait>; // 8K token budget
        let qa_engine = Arc::new(QAEngine::new(llm_client)) as Arc<dyn QAEngineTrait>;
        let metrics_service = Metrics::new();

        let conversational_qa_service = Arc::new(ConversationalQAService::new(
            conversation_service.clone(),
            context_manager,
            qa_engine,
            Arc::new(metrics_service),
        )) as Arc<dyn ConversationalQAServiceTrait>;

        // === Build Use Cases ===

        // Conversation use cases
        use crate::features::conversation::use_cases::*;
        let create_conversation_use_case =
            Arc::new(CreateConversationUseCase::new(conversation_service.clone()));
        let list_conversations_use_case =
            Arc::new(ListConversationsUseCase::new(conversation_service.clone()));
        let get_conversation_use_case =
            Arc::new(GetConversationUseCase::new(conversation_service.clone()));
        let get_conversation_messages_use_case = Arc::new(GetConversationMessagesUseCase::new(
            conversation_service.clone(),
        ));
        let rename_conversation_use_case =
            Arc::new(RenameConversationUseCase::new(conversation_service.clone()));
        let delete_conversation_use_case =
            Arc::new(DeleteConversationUseCase::new(conversation_service.clone()));

        // LLM/Model use cases
        use crate::features::llm::use_cases::*;

        // Use system_info for GetSystemCapabilitiesUseCase
        let get_system_capabilities_use_case =
            Arc::new(GetSystemCapabilitiesUseCase::new(system_info.clone()));

        // Cast DownloadedModelRepository as ModelStoragePort
        let model_storage = downloaded_model_repo.clone() as Arc<dyn ModelStoragePort>;

        let get_available_models_use_case =
            Arc::new(GetAvailableModelsUseCase::new(model_catalog.clone()));
        let get_recommended_models_use_case = Arc::new(GetRecommendedModelsUseCase::new(
            get_system_capabilities_use_case.clone(),
            model_catalog.clone(),
        ));
        let get_best_model_use_case = Arc::new(GetBestModelUseCase::new(
            get_recommended_models_use_case.clone(),
        ));
        let check_model_downloaded_use_case =
            Arc::new(CheckModelDownloadedUseCase::new(model_storage.clone()));
        let get_model_path_use_case = Arc::new(GetModelPathUseCase::new(
            check_model_downloaded_use_case.clone(),
            model_storage.clone(),
        ));
        // TODO: DownloadModelUseCase requires many dependencies not available in AIModule
        // For now, create a stub that returns "not implemented" error
        // Full implementation should be in a separate DownloadModule
        use crate::infrastructure::adapters::fs::tokio_checksum::TokioChecksumAdapter;
        use crate::features::download::download_repository::SqliteDownloadRepository;
        use crate::infrastructure::persistence::repositories::unit_of_work::SqliteUnitOfWorkFactory;
        use crate::features::download::engine::HttpDownloadEngine;
        use crate::features::download::manager::DownloadManagerService;

        let download_repository = Arc::new(SqliteDownloadRepository::new(core.db_conn().clone()));
        let download_engine = Arc::new(HttpDownloadEngine::new()?);
        let download_manager = Arc::new(DownloadManagerService::new(
            download_repository,
            download_engine,
            core.data_dir().clone(),
        ))
            as Arc<dyn crate::features::download::manager::DownloadManager>;
        let file_system = Arc::new(crate::infrastructure::file_system::FileSystemAdapter::new())
            as Arc<dyn crate::application::ports::FileSystemPort>;
        let checksum_service = Arc::new(TokioChecksumAdapter)
            as Arc<dyn crate::domain::ports::file_access::ChecksumService>;
        let file_system_access =
            Arc::new(crate::infrastructure::adapters::fs::TokioFileSystemAdapter)
                as Arc<dyn crate::domain::ports::file_access::FileSystemAccess>;
        let uow_factory = Arc::new(SqliteUnitOfWorkFactory::new(core.db_pool().clone()))
            as Arc<dyn crate::domain::repositories::UnitOfWorkFactory>;

        let download_model_use_case = Arc::new(DownloadModelUseCase::new(
            model_catalog.clone(),
            model_storage.clone(),
            download_manager.clone(),
            file_system,
            core.credentials().clone(),
            checksum_service,
            file_system_access,
            uow_factory,
        ));
        let delete_model_use_case = Arc::new(DeleteModelUseCase::new(model_storage.clone()));
        let list_models_use_case =
            Arc::new(ListDownloadedModelsUseCase::new(model_storage.clone()));

        // AI-powered tags use cases - need TagService
        use crate::features::tags::use_cases::AutoTagAllDocumentsUseCase;
        use crate::features::tags::use_cases::GenerateTagsUseCase;
        use crate::features::tags::service_impl::TagServiceImpl;
        let tag_service = Arc::new(TagServiceImpl::new(db_pool.clone(), llm_cache.clone()))
            as Arc<dyn TagServiceTrait>;

        let generate_tags_use_case = Arc::new(GenerateTagsUseCase::new(tag_service.clone()));
        let auto_tag_all_documents_use_case =
            Arc::new(AutoTagAllDocumentsUseCase::new(tag_service.clone()));

        Ok(Self {
            create_conversation_use_case,
            list_conversations_use_case,
            get_conversation_use_case,
            get_conversation_messages_use_case,
            rename_conversation_use_case,
            delete_conversation_use_case,
            get_system_capabilities_use_case,
            get_available_models_use_case,
            get_recommended_models_use_case,
            get_best_model_use_case,
            check_model_downloaded_use_case,
            get_model_path_use_case,
            download_model_use_case,
            delete_model_use_case,
            list_models_use_case,
            generate_tags_use_case,
            auto_tag_all_documents_use_case,
            conversation_service,
            conversational_qa_service,
            llm_cache,
            inference_engine_cache,
            downloaded_model_repo,
            model_catalog,
            model_catalog_cache,
            credentials,
            download_manager,
        })
    }

    // Conversation use case getters
    pub fn create_conversation_use_case(&self) -> &Arc<CreateConversationUseCase> {
        &self.create_conversation_use_case
    }

    pub fn list_conversations_use_case(&self) -> &Arc<ListConversationsUseCase> {
        &self.list_conversations_use_case
    }

    pub fn get_conversation_use_case(&self) -> &Arc<GetConversationUseCase> {
        &self.get_conversation_use_case
    }

    pub fn get_conversation_messages_use_case(&self) -> &Arc<GetConversationMessagesUseCase> {
        &self.get_conversation_messages_use_case
    }

    pub fn rename_conversation_use_case(&self) -> &Arc<RenameConversationUseCase> {
        &self.rename_conversation_use_case
    }

    pub fn delete_conversation_use_case(&self) -> &Arc<DeleteConversationUseCase> {
        &self.delete_conversation_use_case
    }

    // LLM use case getters
    pub fn get_system_capabilities_use_case(&self) -> &Arc<GetSystemCapabilitiesUseCase> {
        &self.get_system_capabilities_use_case
    }

    pub fn get_available_models_use_case(&self) -> &Arc<GetAvailableModelsUseCase> {
        &self.get_available_models_use_case
    }

    pub fn get_recommended_models_use_case(&self) -> &Arc<GetRecommendedModelsUseCase> {
        &self.get_recommended_models_use_case
    }

    pub fn get_best_model_use_case(&self) -> &Arc<GetBestModelUseCase> {
        &self.get_best_model_use_case
    }

    pub fn check_model_downloaded_use_case(&self) -> &Arc<CheckModelDownloadedUseCase> {
        &self.check_model_downloaded_use_case
    }

    pub fn get_model_path_use_case(&self) -> &Arc<GetModelPathUseCase> {
        &self.get_model_path_use_case
    }

    pub fn download_model_use_case(&self) -> &Arc<DownloadModelUseCase> {
        &self.download_model_use_case
    }

    pub fn delete_model_use_case(&self) -> &Arc<DeleteModelUseCase> {
        &self.delete_model_use_case
    }

    pub fn list_models_use_case(&self) -> &Arc<ListDownloadedModelsUseCase> {
        &self.list_models_use_case
    }

    // AI-powered tag getters
    pub fn generate_tags_use_case(&self) -> &Arc<GenerateTagsUseCase> {
        &self.generate_tags_use_case
    }

    pub fn auto_tag_all_documents_use_case(&self) -> &Arc<AutoTagAllDocumentsUseCase> {
        &self.auto_tag_all_documents_use_case
    }

    // Service getters
    pub fn conversation_service(&self) -> &Arc<dyn ConversationServiceTrait> {
        &self.conversation_service
    }

    pub fn conversational_qa_service(&self) -> &Arc<dyn ConversationalQAServiceTrait> {
        &self.conversational_qa_service
    }

    pub fn downloaded_model_repo(
        &self,
    ) -> &Arc<crate::infrastructure::persistence::repositories::DownloadedModelRepository> {
        &self.downloaded_model_repo
    }

    // Cache getters (for Container lazy loading)
    pub fn llm_cache(&self) -> &Arc<RwLock<Option<Arc<dyn LLMPort>>>> {
        &self.llm_cache
    }

    pub fn inference_engine_cache(&self) -> &Arc<RwLock<Option<Arc<InferenceEngine>>>> {
        &self.inference_engine_cache
    }

    // Additional getters needed by Container
    pub fn model_catalog(&self) -> &Arc<dyn ModelCatalogPort> {
        &self.model_catalog
    }

    pub fn model_catalog_cache(
        &self,
    ) -> &Arc<crate::infrastructure::model_cache_adapter::ModelCacheAdapter> {
        &self.model_catalog_cache
    }

    pub fn download_manager(
        &self,
    ) -> &Arc<dyn crate::features::download::manager::DownloadManager> {
        &self.download_manager
    }
}

// ==============================================================================
// 5. LibraryModule - Tags, Favorites, Mentions
// ==============================================================================

/// Library module for document organization (tags, favorites, mentions)
///
/// **Fields**: ~20
/// - 18 use cases (tags, favorites, recent, mentions)
/// - 1 service (tag service)
/// - 4 repositories
#[derive(Clone)]
pub struct LibraryModule {
    // Use Cases - Tags
    create_tag_use_case: Arc<CreateTagUseCase>,
    update_tag_use_case: Arc<UpdateTagUseCase>,
    delete_tag_use_case: Arc<DeleteTagUseCase>,
    remove_tag_from_document_use_case: Arc<RemoveTagFromDocumentUseCase>,
    get_tags_use_case: Arc<GetTagsUseCase>,
    apply_tags_use_case: Arc<ApplyTagsUseCase>,
    search_by_tag_use_case: Arc<SearchByTagUseCase>,

    // Use Cases - Favorites
    add_favorite_use_case: Arc<AddFavoriteUseCase>,
    remove_favorite_use_case: Arc<RemoveFavoriteUseCase>,
    list_favorites_use_case: Arc<ListFavoritesUseCase>,
    is_favorite_use_case: Arc<IsFavoriteUseCase>,

    // Use Cases - Recent
    track_access_use_case: Arc<TrackAccessUseCase>,
    get_recent_documents_use_case: Arc<GetRecentDocumentsUseCase>,
    clear_recent_history_use_case: Arc<ClearRecentHistoryUseCase>,

    // Use Cases - Mentions
    extract_mentions_use_case: Arc<ExtractMentionsUseCase>,
    search_mentions_use_case: Arc<SearchMentionsUseCase>,
    get_backlinks_use_case: Arc<GetBacklinksUseCase>,
    get_mentions_by_type_use_case: Arc<GetMentionsByTypeUseCase>,
    get_mentions_for_document_use_case: Arc<GetMentionsForDocumentUseCase>,
    create_mention_use_case: Arc<CreateMentionUseCase>,
    update_mention_use_case: Arc<UpdateMentionUseCase>,
    delete_mention_use_case: Arc<DeleteMentionUseCase>,

    // Services
    tag_service: Arc<dyn TagServiceTrait>,

    // Repositories
    document_repo: Arc<dyn DocumentRepository>,
    favorites_repo: Arc<dyn FavoritesRepositoryPort>,
    recent_docs_repo: Arc<dyn RecentDocumentsRepositoryPort>,
    mention_repo: Arc<dyn MentionRepositoryPort>,
}

impl LibraryModule {
    /// Build LibraryModule with all its dependencies
    ///
    /// Constructs all repositories, services, and use cases for document library features:
    /// - Tags (create, update, delete, search by tag)
    /// - Favorites (add, remove, list, check)
    /// - Recent documents (track, list, clear)
    /// - Mentions (extract, search, backlinks)
    pub async fn new(
        db_pool: SqlitePool,
        _core: Arc<CoreModule>,
    ) -> crate::shared::error::Result<Self> {
        // === Build Repositories ===

        // Document Repository (for tag operations)
        use crate::infrastructure::persistence::repositories::DocumentRepositoryImpl;
        let document_repo =
            Arc::new(DocumentRepositoryImpl::new(db_pool.clone())) as Arc<dyn DocumentRepository>;

        // Favorites Repository
        use crate::features::favorites::repository::FavoritesRepository;
        let favorites_repo =
            Arc::new(FavoritesRepository::new(db_pool.clone())) as Arc<dyn FavoritesRepositoryPort>;

        // Recent Documents Repository
        use crate::features::recent::repository::RecentDocumentsRepository;
        let recent_docs_repo = Arc::new(RecentDocumentsRepository::new(db_pool.clone()))
            as Arc<dyn RecentDocumentsRepositoryPort>;

        // Mention Repository
        use crate::infrastructure::persistence::repositories::MentionRepository;
        let mention_repo =
            Arc::new(MentionRepository::new(db_pool.clone())) as Arc<dyn MentionRepositoryPort>;

        // === Build Services ===

        // Tag Service
        use crate::features::tags::service::TagService;
        let tag_service = Arc::new(TagService::new(db_pool.clone())) as Arc<dyn TagServiceTrait>;

        // === Build Use Cases ===

        // Tag use cases
        use crate::features::tags::use_cases::*;
        let create_tag_use_case = Arc::new(CreateTagUseCase::new(tag_service.clone()));
        let update_tag_use_case = Arc::new(UpdateTagUseCase::new(tag_service.clone()));
        let delete_tag_use_case = Arc::new(DeleteTagUseCase::new(tag_service.clone()));
        let remove_tag_from_document_use_case =
            Arc::new(RemoveTagFromDocumentUseCase::new(tag_service.clone()));
        let get_tags_use_case = Arc::new(GetTagsUseCase::new(tag_service.clone()));
        let apply_tags_use_case = Arc::new(ApplyTagsUseCase::new(tag_service.clone()));
        let search_by_tag_use_case = Arc::new(SearchByTagUseCase::new(tag_service.clone()));

        // Favorites use cases
        use crate::features::favorites::use_cases::*;
        let add_favorite_use_case = Arc::new(AddFavoriteUseCase::new(favorites_repo.clone()));
        let remove_favorite_use_case = Arc::new(RemoveFavoriteUseCase::new(favorites_repo.clone()));
        let list_favorites_use_case = Arc::new(ListFavoritesUseCase::new(favorites_repo.clone()));
        let is_favorite_use_case = Arc::new(IsFavoriteUseCase::new(favorites_repo.clone()));

        // Recent documents use cases
        use crate::features::recent::use_cases::*;
        let track_access_use_case = Arc::new(TrackAccessUseCase::new(recent_docs_repo.clone()));
        let get_recent_documents_use_case =
            Arc::new(GetRecentDocumentsUseCase::new(recent_docs_repo.clone()));
        let clear_recent_history_use_case =
            Arc::new(ClearRecentHistoryUseCase::new(recent_docs_repo.clone()));

        // Mentions use cases - need MentionMapper
        use crate::features::mentions::mapper::MentionMapper;
        use crate::features::mentions::use_cases::{
            CreateMentionUseCase, DeleteMentionUseCase, ExtractMentionsUseCase,
            GetBacklinksUseCase, GetMentionsByTypeUseCase, GetMentionsForDocumentUseCase,
            SearchMentionsUseCase, UpdateMentionUseCase,
        };
        let mention_mapper = Arc::new(MentionMapper::new());

        let extract_mentions_use_case = Arc::new(ExtractMentionsUseCase::new(
            mention_repo.clone(),
            mention_mapper.clone(),
        ));
        let search_mentions_use_case = Arc::new(SearchMentionsUseCase::new(
            mention_repo.clone(),
            mention_mapper.clone(),
        ));
        let get_backlinks_use_case = Arc::new(GetBacklinksUseCase::new(mention_repo.clone()));
        let get_mentions_by_type_use_case = Arc::new(GetMentionsByTypeUseCase::new(
            mention_repo.clone(),
            mention_mapper.clone(),
        ));
        let get_mentions_for_document_use_case = Arc::new(GetMentionsForDocumentUseCase::new(
            mention_repo.clone(),
            mention_mapper.clone(),
        ));
        let create_mention_use_case = Arc::new(CreateMentionUseCase::new(
            mention_repo.clone(),
            mention_mapper.clone(),
        ));
        let update_mention_use_case = Arc::new(UpdateMentionUseCase::new(
            mention_repo.clone(),
            mention_mapper.clone(),
        ));
        let delete_mention_use_case = Arc::new(DeleteMentionUseCase::new(mention_repo.clone()));

        Ok(Self {
            create_tag_use_case,
            update_tag_use_case,
            delete_tag_use_case,
            remove_tag_from_document_use_case,
            get_tags_use_case,
            apply_tags_use_case,
            search_by_tag_use_case,
            add_favorite_use_case,
            remove_favorite_use_case,
            list_favorites_use_case,
            is_favorite_use_case,
            track_access_use_case,
            get_recent_documents_use_case,
            clear_recent_history_use_case,
            extract_mentions_use_case,
            search_mentions_use_case,
            get_backlinks_use_case,
            get_mentions_by_type_use_case,
            get_mentions_for_document_use_case,
            create_mention_use_case,
            update_mention_use_case,
            delete_mention_use_case,
            tag_service,
            document_repo,
            favorites_repo,
            recent_docs_repo,
            mention_repo,
        })
    }

    // Tag use case getters
    pub fn create_tag_use_case(&self) -> &Arc<CreateTagUseCase> {
        &self.create_tag_use_case
    }

    pub fn update_tag_use_case(&self) -> &Arc<UpdateTagUseCase> {
        &self.update_tag_use_case
    }

    pub fn delete_tag_use_case(&self) -> &Arc<DeleteTagUseCase> {
        &self.delete_tag_use_case
    }

    pub fn remove_tag_from_document_use_case(&self) -> &Arc<RemoveTagFromDocumentUseCase> {
        &self.remove_tag_from_document_use_case
    }

    pub fn get_tags_use_case(&self) -> &Arc<GetTagsUseCase> {
        &self.get_tags_use_case
    }

    pub fn apply_tags_use_case(&self) -> &Arc<ApplyTagsUseCase> {
        &self.apply_tags_use_case
    }

    pub fn search_by_tag_use_case(&self) -> &Arc<SearchByTagUseCase> {
        &self.search_by_tag_use_case
    }

    // Favorites use case getters
    pub fn add_favorite_use_case(&self) -> &Arc<AddFavoriteUseCase> {
        &self.add_favorite_use_case
    }

    pub fn remove_favorite_use_case(&self) -> &Arc<RemoveFavoriteUseCase> {
        &self.remove_favorite_use_case
    }

    pub fn list_favorites_use_case(&self) -> &Arc<ListFavoritesUseCase> {
        &self.list_favorites_use_case
    }

    pub fn is_favorite_use_case(&self) -> &Arc<IsFavoriteUseCase> {
        &self.is_favorite_use_case
    }

    // Recent use case getters
    pub fn track_access_use_case(&self) -> &Arc<TrackAccessUseCase> {
        &self.track_access_use_case
    }

    pub fn get_recent_documents_use_case(&self) -> &Arc<GetRecentDocumentsUseCase> {
        &self.get_recent_documents_use_case
    }

    pub fn clear_recent_history_use_case(&self) -> &Arc<ClearRecentHistoryUseCase> {
        &self.clear_recent_history_use_case
    }

    // Mentions use case getters
    pub fn extract_mentions_use_case(&self) -> &Arc<ExtractMentionsUseCase> {
        &self.extract_mentions_use_case
    }

    pub fn search_mentions_use_case(&self) -> &Arc<SearchMentionsUseCase> {
        &self.search_mentions_use_case
    }

    pub fn get_backlinks_use_case(&self) -> &Arc<GetBacklinksUseCase> {
        &self.get_backlinks_use_case
    }

    pub fn get_mentions_by_type_use_case(&self) -> &Arc<GetMentionsByTypeUseCase> {
        &self.get_mentions_by_type_use_case
    }

    pub fn get_mentions_for_document_use_case(&self) -> &Arc<GetMentionsForDocumentUseCase> {
        &self.get_mentions_for_document_use_case
    }

    pub fn favorites_repo(&self) -> &Arc<dyn FavoritesRepositoryPort> {
        &self.favorites_repo
    }

    pub fn recent_docs_repo(&self) -> &Arc<dyn RecentDocumentsRepositoryPort> {
        &self.recent_docs_repo
    }

    pub fn create_mention_use_case(&self) -> &Arc<CreateMentionUseCase> {
        &self.create_mention_use_case
    }

    pub fn update_mention_use_case(&self) -> &Arc<UpdateMentionUseCase> {
        &self.update_mention_use_case
    }

    pub fn delete_mention_use_case(&self) -> &Arc<DeleteMentionUseCase> {
        &self.delete_mention_use_case
    }

    // Service getters
    pub fn tag_service(&self) -> &Arc<dyn TagServiceTrait> {
        &self.tag_service
    }

    // Repository getters needed by Container
    pub fn mention_repo(&self) -> &Arc<dyn MentionRepositoryPort> {
        &self.mention_repo
    }
}

// ==============================================================================
// 6. FileOpsModule - File System Operations
// ==============================================================================

/// File operations module for opening files, metadata, wikilinks
///
/// **Fields**: ~10
/// - 9 use cases (file operations, extraction)
/// - File system adapter
/// - Document repository (read-only for paths)
#[derive(Clone)]
pub struct FileOpsModule {
    // Use Cases - File Operations
    open_file_use_case: Arc<OpenFileUseCase>,
    open_file_by_id_use_case: Arc<OpenFileByIdUseCase>,
    get_file_path_by_id_use_case: Arc<GetFilePathByIdUseCase>,
    show_in_folder_use_case: Arc<ShowInFolderUseCase>,
    get_file_metadata_use_case: Arc<GetFileMetadataUseCase>,
    read_file_content_use_case: Arc<ReadFileContentUseCase>,
    read_file_bytes_use_case: Arc<ReadFileBytesUseCase>,
    update_file_metadata_use_case: Arc<UpdateFileMetadataUseCase>,

    // Use Cases - Extraction
    parse_wikilinks_use_case: Arc<ParseWikilinksUseCase>,
    extract_document_title_use_case: Arc<ExtractDocumentTitleUseCase>,
    resolve_wikilink_use_case: Arc<ResolveWikilinkUseCase>,
    extract_and_resolve_links_use_case: Arc<ExtractAndResolveLinksUseCase>,

    // Adapters
    file_system: Arc<dyn FileSystemPort>,

    // Repositories
    document_repo: Arc<dyn DocumentRepository>,
}

impl FileOpsModule {
    /// Build FileOpsModule with all its dependencies
    ///
    /// Constructs repositories, adapters, and use cases for file operations:
    /// - File operations (open, show in folder, get metadata, read content)
    /// - Wikilink operations (parse, extract title, resolve)
    pub async fn new(
        db_pool: SqlitePool,
        core: Arc<CoreModule>,
    ) -> crate::shared::error::Result<Self> {
        // === Build Adapters ===

        // File System adapter (uses security context from core)
        use crate::infrastructure::file_system::FileSystemAdapter;
        let file_system = Arc::new(FileSystemAdapter::new()) as Arc<dyn FileSystemPort>;

        // File Storage adapter - use SecureFileStorage
        use crate::infrastructure::file_system::file_storage::SecureFileStorage;
        let file_storage = Arc::new(SecureFileStorage::new()) as Arc<dyn FileStoragePort>;

        // Get FileAccessConfig from core
        let file_access_config = core.file_access_config().clone();

        // Get vault path from core data_dir (parent of data_dir)
        let vault_path = core
            .data_dir()
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| core.data_dir().clone());

        // === Build Repositories ===

        // Document Repository
        use crate::infrastructure::persistence::repositories::DocumentRepositoryImpl;
        let document_repo =
            Arc::new(DocumentRepositoryImpl::new(db_pool.clone())) as Arc<dyn DocumentRepository>;

        use crate::features::tags::service::TagService;
        let tag_service = Arc::new(TagService::new(db_pool.clone())) as Arc<dyn TagServiceTrait>;

        // === Build Use Cases ===

        // File operations use cases
        use crate::features::file::use_cases::*;
        let open_file_use_case = Arc::new(OpenFileUseCase::new(
            file_system.clone(),
            file_storage.clone(),
            file_access_config.clone(),
        ));
        let open_file_by_id_use_case = Arc::new(OpenFileByIdUseCase::new(
            document_repo.clone(),
            file_system.clone(),
            file_storage.clone(),
            file_access_config.clone(),
            vault_path.clone(),
        ));
        let get_file_path_by_id_use_case = Arc::new(GetFilePathByIdUseCase::new(
            document_repo.clone(),
            file_storage.clone(),
            file_access_config.clone(),
            vault_path.clone(),
        ));
        let show_in_folder_use_case = Arc::new(ShowInFolderUseCase::new(
            file_system.clone(),
            file_storage.clone(),
            file_access_config.clone(),
        ));
        let get_file_metadata_use_case = Arc::new(GetFileMetadataUseCase::new(
            file_storage.clone(),
            file_access_config.clone(),
        ));
        let read_file_content_use_case = Arc::new(ReadFileContentUseCase::new(
            file_storage.clone(),
            file_access_config.clone(),
        ));
        let read_file_bytes_use_case = Arc::new(ReadFileBytesUseCase::new(
            file_storage.clone(),
            file_access_config.clone(),
        ));
        let update_file_metadata_use_case = Arc::new(UpdateFileMetadataUseCase::new(
            document_repo.clone() as Arc<dyn crate::application::ports::DocumentRepositoryPort>,
            tag_service.clone(),
        ));

        // Extraction use cases
        use crate::features::extraction::use_cases::*;
        let parse_wikilinks_use_case = Arc::new(ParseWikilinksUseCase::new());
        let extract_document_title_use_case = Arc::new(ExtractDocumentTitleUseCase::new());
        let resolve_wikilink_use_case = Arc::new(ResolveWikilinkUseCase::new());

        // ExtractAndResolveLinksUseCase needs parse, resolve, and document repo
        use crate::domain::entities::Document;
        let extract_and_resolve_links_use_case = Arc::new(ExtractAndResolveLinksUseCase::new(
            Arc::clone(&parse_wikilinks_use_case) as Arc<dyn ParseWikilinksPort>,
            Arc::clone(&resolve_wikilink_use_case) as Arc<dyn ResolveWikilinkPort>,
            Arc::clone(&document_repo) as Arc<dyn RepositoryPort<Document>>,
        ));

        Ok(Self {
            open_file_use_case,
            open_file_by_id_use_case,
            get_file_path_by_id_use_case,
            show_in_folder_use_case,
            get_file_metadata_use_case,
            read_file_content_use_case,
            read_file_bytes_use_case,
            update_file_metadata_use_case,
            parse_wikilinks_use_case,
            extract_document_title_use_case,
            resolve_wikilink_use_case,
            extract_and_resolve_links_use_case,
            file_system,
            document_repo,
        })
    }

    // File operation use case getters
    pub fn open_file_use_case(&self) -> &Arc<OpenFileUseCase> {
        &self.open_file_use_case
    }

    pub fn open_file_by_id_use_case(&self) -> &Arc<OpenFileByIdUseCase> {
        &self.open_file_by_id_use_case
    }

    pub fn get_file_path_by_id_use_case(&self) -> &Arc<GetFilePathByIdUseCase> {
        &self.get_file_path_by_id_use_case
    }

    pub fn show_in_folder_use_case(&self) -> &Arc<ShowInFolderUseCase> {
        &self.show_in_folder_use_case
    }

    pub fn get_file_metadata_use_case(&self) -> &Arc<GetFileMetadataUseCase> {
        &self.get_file_metadata_use_case
    }

    pub fn read_file_content_use_case(&self) -> &Arc<ReadFileContentUseCase> {
        &self.read_file_content_use_case
    }

    pub fn read_file_bytes_use_case(&self) -> &Arc<ReadFileBytesUseCase> {
        &self.read_file_bytes_use_case
    }

    pub fn update_file_metadata_use_case(&self) -> &Arc<UpdateFileMetadataUseCase> {
        &self.update_file_metadata_use_case
    }

    // Extraction use case getters
    pub fn parse_wikilinks_use_case(&self) -> &Arc<ParseWikilinksUseCase> {
        &self.parse_wikilinks_use_case
    }

    pub fn extract_document_title_use_case(&self) -> &Arc<ExtractDocumentTitleUseCase> {
        &self.extract_document_title_use_case
    }

    pub fn resolve_wikilink_use_case(&self) -> &Arc<ResolveWikilinkUseCase> {
        &self.resolve_wikilink_use_case
    }

    pub fn extract_and_resolve_links_use_case(&self) -> &Arc<ExtractAndResolveLinksUseCase> {
        &self.extract_and_resolve_links_use_case
    }
}

// ==============================================================================
// 7. SystemModule - Settings, Health, Backup, Cache
// ==============================================================================

/// System module for application-wide operations
///
/// **Fields**: ~15
/// - 17 use cases (health, settings, cache, backup, updates, metrics, stats)
/// - 5 adapters
/// - Settings repository
#[derive(Clone)]
pub struct SystemModule {
    // Use Cases - Health
    health_check_use_case: Arc<HealthCheckUseCase>,
    initialize_database_use_case: Arc<InitializeDatabaseUseCase>,
    initialize_models_use_case: Arc<InitializeModelsUseCase>,

    // Use Cases - Settings
    get_settings_use_case: Arc<GetSettingsUseCase>,
    update_settings_use_case: Arc<UpdateSettingsUseCase>,
    reset_settings_use_case: Arc<ResetSettingsUseCase>,
    export_settings_use_case: Arc<ExportSettingsUseCase>,
    import_settings_use_case: Arc<ImportSettingsUseCase>,
    validate_settings_use_case: Arc<ValidateSettingsUseCase>,

    // Use Cases - Cache
    clear_cache_use_case: Arc<ClearCacheUseCase>,
    get_cache_size_use_case: Arc<GetCacheSizeUseCase>,
    get_cache_stats_use_case: Arc<GetCacheStatsUseCase>,

    // Use Cases - Backup
    create_backup_use_case: Arc<CreateBackupUseCase>,
    restore_backup_use_case: Arc<RestoreBackupUseCase>,
    list_backups_use_case: Arc<ListBackupsUseCase>,
    start_auto_backup_use_case: Arc<StartAutoBackupUseCase>,
    stop_auto_backup_use_case: Arc<StopAutoBackupUseCase>,
    startup_auto_backup_use_case: Arc<StartupAutoBackupUseCase>,

    // Backup scheduler
    backup_scheduler: Arc<crate::features::backup::scheduler::BackupScheduler>,

    // Use Cases - Updates
    check_for_updates_use_case: Arc<CheckForUpdatesUseCase>,
    get_current_version_use_case: Arc<GetCurrentVersionUseCase>,

    // Use Cases - Metrics
    get_metrics_use_case: Arc<GetMetricsUseCase>,

    // Use Cases - Stats
    get_system_stats_use_case: Arc<GetSystemStatsUseCase>,

    // Adapters
    cache: Arc<dyn CachePort>,
    backup: Arc<dyn BackupPort>,
    update_checker: Arc<dyn UpdateCheckerPort>,
    metrics: Arc<dyn MetricsPort>,
    system_info: Arc<dyn SystemInfoPort>,
    metrics_service: Arc<Metrics>,

    // Repositories
    settings_repo: Arc<dyn SettingsRepositoryPort>,
}

impl SystemModule {
    /// Build SystemModule with its own dependencies - Layer 2
    ///
    /// Constructs system-wide infrastructure:
    /// - Settings repository
    /// - Cache, backup, updates, metrics adapters
    /// - System health and stats use cases
    ///
    /// Stack frame is freed after return, independent of other modules.
    pub async fn new(
        db_pool: SqlitePool,
        core: Arc<CoreModule>,
    ) -> crate::shared::error::Result<Self> {
        // === Build System Infrastructure ===

        // Settings Repository (JSON file-based)
        use crate::infrastructure::persistence::repositories::SettingsRepository;
        let settings_path = core.data_dir().join("settings.json");
        let settings_repo = Arc::new(SettingsRepository::new(settings_path).await?)
            as Arc<dyn SettingsRepositoryPort>;

        // Cache Adapter (LLM response caching)
        use crate::features::cache::adapter::CacheAdapter;
        use crate::features::cache::llm_cache::LlmCache;
        let llm_cache = LlmCache::new();
        let cache = Arc::new(CacheAdapter::new(llm_cache)) as Arc<dyn CachePort>;

        // Backup Adapter (database backups)
        use crate::features::backup::adapter::BackupAdapter;
        let db_path = core.data_dir().join("vault.db");
        let backup = Arc::new(BackupAdapter::new(db_pool.clone(), db_path)) as Arc<dyn BackupPort>;

        // Update Checker Adapter
        use crate::features::updates::adapter::UpdateCheckerAdapter;
        let update_checker = Arc::new(UpdateCheckerAdapter::new()) as Arc<dyn UpdateCheckerPort>;

        // Metrics Adapter
        use crate::infrastructure::observability::metrics::Metrics;
        use crate::features::metrics::adapter::MetricsAdapter;
        let metrics_service = Arc::new(Metrics::new());
        let metrics =
            Arc::new(MetricsAdapter::new(metrics_service.as_ref().clone())) as Arc<dyn MetricsPort>;

        // System Info Adapter
        use crate::infrastructure::system_info_adapter::SystemInfoAdapter;
        let system_info = Arc::new(SystemInfoAdapter::new()) as Arc<dyn SystemInfoPort>;

        // === Build Repos for Stats (read-only counts) ===
        use crate::application::ports::RepositoryPort;
        use crate::domain::entities::{Chunk, Document as DocumentEntity};
        use crate::features::tags::entity::Tag as TagEntity;
        use crate::infrastructure::persistence::repositories::{
            ChunkRepositoryImpl, DocumentRepositoryImpl, TagRepositoryImpl,
        };

        let document_repo = Arc::new(DocumentRepositoryImpl::new(db_pool.clone()))
            as Arc<dyn RepositoryPort<DocumentEntity>>;
        let chunk_repo =
            Arc::new(ChunkRepositoryImpl::new(db_pool.clone())) as Arc<dyn RepositoryPort<Chunk>>;
        let tag_repo =
            Arc::new(TagRepositoryImpl::new(db_pool.clone())) as Arc<dyn RepositoryPort<TagEntity>>;

        // === Build Use Cases ===

        // Health Check (needs embedding + LLM - use mocks for now)
        let models_path = core.data_dir().join("models");
        let model_manager = Arc::new(ModelManager::new(models_path)?) as Arc<dyn ModelManagerTrait>;
        let initialize_database_use_case =
            Arc::new(InitializeDatabaseUseCase::new(Arc::new(db_pool.clone())));
        let initialize_models_use_case = Arc::new(InitializeModelsUseCase::new(model_manager));

        use crate::application::ports::MockEmbeddingPort;
        let health_embedding =
            Arc::new(MockEmbeddingPort::new_degraded()) as Arc<dyn EmbeddingPort>;
        use crate::infrastructure::llm::factory::MockLLMPort;
        let health_llm = Arc::new(MockLLMPort::default()) as Arc<dyn LLMPort>;

        let health_check_use_case = Arc::new(HealthCheckUseCase::new(
            db_pool.clone(),
            health_embedding,
            health_llm,
        ));

        // Stats
        let database_stats = Arc::new(
            crate::features::stats::database_stats::DatabaseStatsAdapter::new(
                db_pool.clone(),
            ),
        ) as Arc<dyn crate::application::ports::DatabaseStatsPort>;
        let get_system_stats_use_case = Arc::new(GetSystemStatsUseCase::new(
            document_repo,
            chunk_repo,
            tag_repo,
            database_stats,
        ));

        // Settings
        let get_settings_use_case = Arc::new(GetSettingsUseCase::new(settings_repo.clone()));
        let update_settings_use_case = Arc::new(UpdateSettingsUseCase::new(settings_repo.clone()));
        let reset_settings_use_case = Arc::new(ResetSettingsUseCase::new(settings_repo.clone()));
        let export_settings_use_case = Arc::new(ExportSettingsUseCase::new(settings_repo.clone()));
        let import_settings_use_case = Arc::new(ImportSettingsUseCase::new(settings_repo.clone()));
        let validate_settings_use_case =
            Arc::new(ValidateSettingsUseCase::new(settings_repo.clone()));

        // Cache
        let clear_cache_use_case = Arc::new(ClearCacheUseCase::new(cache.clone()));
        let get_cache_size_use_case = Arc::new(GetCacheSizeUseCase::new(cache.clone()));
        let get_cache_stats_use_case = Arc::new(GetCacheStatsUseCase::new(cache.clone()));

        // Backup
        let create_backup_use_case = Arc::new(CreateBackupUseCase::new(backup.clone()));
        let restore_backup_use_case = Arc::new(RestoreBackupUseCase::new(backup.clone()));
        let list_backups_use_case = Arc::new(ListBackupsUseCase::new(backup.clone()));
        let backup_scheduler = Arc::new(crate::features::backup::scheduler::BackupScheduler::new(
            create_backup_use_case.clone(),
            settings_repo.clone(),
        ));
        let backup_scheduler_port: Arc<dyn BackupSchedulerPort> = backup_scheduler.clone();
        let start_auto_backup_use_case = Arc::new(StartAutoBackupUseCase::new(
            settings_repo.clone(),
            backup_scheduler_port.clone(),
        ));
        let stop_auto_backup_use_case = Arc::new(StopAutoBackupUseCase::new(
            settings_repo.clone(),
            backup_scheduler_port.clone(),
        ));
        let startup_auto_backup_use_case = Arc::new(StartupAutoBackupUseCase::new(
            settings_repo.clone(),
            backup_scheduler_port.clone(),
        ));

        // Updates
        let check_for_updates_use_case =
            Arc::new(CheckForUpdatesUseCase::new(update_checker.clone()));
        let get_current_version_use_case =
            Arc::new(GetCurrentVersionUseCase::new(update_checker.clone()));

        // Metrics
        let get_metrics_use_case = Arc::new(GetMetricsUseCase::new(metrics.clone()));

        Ok(Self {
            health_check_use_case,
            initialize_database_use_case,
            initialize_models_use_case,
            get_settings_use_case,
            update_settings_use_case,
            reset_settings_use_case,
            export_settings_use_case,
            import_settings_use_case,
            validate_settings_use_case,
            clear_cache_use_case,
            get_cache_size_use_case,
            get_cache_stats_use_case,
            create_backup_use_case,
            restore_backup_use_case,
            list_backups_use_case,
            start_auto_backup_use_case,
            stop_auto_backup_use_case,
            startup_auto_backup_use_case,
            backup_scheduler,
            check_for_updates_use_case,
            get_current_version_use_case,
            get_metrics_use_case,
            get_system_stats_use_case,
            cache,
            backup,
            update_checker,
            metrics,
            system_info,
            metrics_service,
            settings_repo,
        })
    }

    // Health use case getters
    pub fn health_check_use_case(&self) -> &Arc<HealthCheckUseCase> {
        &self.health_check_use_case
    }

    pub fn initialize_database_use_case(&self) -> &Arc<InitializeDatabaseUseCase> {
        &self.initialize_database_use_case
    }

    pub fn initialize_models_use_case(&self) -> &Arc<InitializeModelsUseCase> {
        &self.initialize_models_use_case
    }

    // Settings use case getters
    pub fn get_settings_use_case(&self) -> &Arc<GetSettingsUseCase> {
        &self.get_settings_use_case
    }

    pub fn update_settings_use_case(&self) -> &Arc<UpdateSettingsUseCase> {
        &self.update_settings_use_case
    }

    pub fn reset_settings_use_case(&self) -> &Arc<ResetSettingsUseCase> {
        &self.reset_settings_use_case
    }

    pub fn export_settings_use_case(&self) -> &Arc<ExportSettingsUseCase> {
        &self.export_settings_use_case
    }

    pub fn import_settings_use_case(&self) -> &Arc<ImportSettingsUseCase> {
        &self.import_settings_use_case
    }

    pub fn validate_settings_use_case(&self) -> &Arc<ValidateSettingsUseCase> {
        &self.validate_settings_use_case
    }

    // Cache use case getters
    pub fn clear_cache_use_case(&self) -> &Arc<ClearCacheUseCase> {
        &self.clear_cache_use_case
    }

    pub fn get_cache_size_use_case(&self) -> &Arc<GetCacheSizeUseCase> {
        &self.get_cache_size_use_case
    }

    pub fn get_cache_stats_use_case(&self) -> &Arc<GetCacheStatsUseCase> {
        &self.get_cache_stats_use_case
    }

    // Backup use case getters
    pub fn create_backup_use_case(&self) -> &Arc<CreateBackupUseCase> {
        &self.create_backup_use_case
    }

    pub fn restore_backup_use_case(&self) -> &Arc<RestoreBackupUseCase> {
        &self.restore_backup_use_case
    }

    pub fn list_backups_use_case(&self) -> &Arc<ListBackupsUseCase> {
        &self.list_backups_use_case
    }

    pub fn start_auto_backup_use_case(&self) -> &Arc<StartAutoBackupUseCase> {
        &self.start_auto_backup_use_case
    }

    pub fn stop_auto_backup_use_case(&self) -> &Arc<StopAutoBackupUseCase> {
        &self.stop_auto_backup_use_case
    }

    pub fn startup_auto_backup_use_case(&self) -> &Arc<StartupAutoBackupUseCase> {
        &self.startup_auto_backup_use_case
    }

    pub fn backup_scheduler(&self) -> &Arc<crate::features::backup::scheduler::BackupScheduler> {
        &self.backup_scheduler
    }

    // Updates use case getters
    pub fn check_for_updates_use_case(&self) -> &Arc<CheckForUpdatesUseCase> {
        &self.check_for_updates_use_case
    }

    pub fn get_current_version_use_case(&self) -> &Arc<GetCurrentVersionUseCase> {
        &self.get_current_version_use_case
    }

    // Metrics use case getters
    pub fn get_metrics_use_case(&self) -> &Arc<GetMetricsUseCase> {
        &self.get_metrics_use_case
    }

    // Stats use case getters
    pub fn get_system_stats_use_case(&self) -> &Arc<GetSystemStatsUseCase> {
        &self.get_system_stats_use_case
    }

    // Adapter getters needed by Container
    pub fn system_info(&self) -> &Arc<dyn SystemInfoPort> {
        &self.system_info
    }

    pub fn metrics_service(&self) -> &Arc<Metrics> {
        &self.metrics_service
    }
}
