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
    ArticleExtractorServiceTrait, ModelManagerTrait, SearchEnrichmentServiceTrait,
};
use crate::features::batch::{BatchFileImportServiceTrait, BatchUrlImportServiceTrait};
use crate::features::conversation::ConversationServiceTrait;
use crate::features::embedding::EmbeddingServiceTrait;
use crate::features::indexing::{IndexStorageTrait, IndexingServiceTrait};
use crate::features::qa::ConversationalQAServiceTrait;
use crate::features::search::{BM25SearchTrait, HybridSearchTrait, SearchServiceTrait};
use crate::features::tags::TagServiceTrait;
use crate::features::web::{WebArchiveServiceTrait, WebCaptureServiceTrait, WebIngestionServiceTrait};

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
    search: crate::features::search::di::SearchDi,
}

impl SearchModule {
    /// Build SearchModule by composing the search feature builder.
    pub async fn new(
        db_pool: SqlitePool,
        core: Arc<CoreModule>,
        embedding_cache: Arc<RwLock<Option<Arc<dyn EmbeddingPort>>>>,
    ) -> crate::shared::error::Result<Self> {
        let usearch_index_path = core.data_dir().join("usearch_index.usearch");
        let search =
            crate::features::search::di::build(db_pool, usearch_index_path, embedding_cache)
                .await?;
        Ok(Self { search })
    }

    // Use case getters
    pub fn semantic_search_use_case(&self) -> &Arc<SemanticSearchUseCase> {
        &self.search.semantic_search_use_case
    }

    pub fn hybrid_search_use_case(&self) -> &Arc<HybridSearchUseCase> {
        &self.search.hybrid_search_use_case
    }

    pub fn file_search_use_case(&self) -> &Arc<FileSearchUseCase> {
        &self.search.file_search_use_case
    }

    pub fn recency_search_use_case(&self) -> &Arc<RecencySearchUseCase> {
        &self.search.recency_search_use_case
    }

    pub fn search_service(&self) -> &Arc<dyn SearchServiceTrait> {
        &self.search.search_service
    }

    pub fn bm25_search_service(&self) -> &Arc<dyn BM25SearchTrait> {
        &self.search.bm25_search
    }

    pub fn hybrid_search_service(&self) -> &Arc<dyn HybridSearchTrait> {
        &self.search.hybrid_search_service
    }

    pub fn search_enrichment_service(&self) -> &Arc<dyn SearchEnrichmentServiceTrait> {
        &self.search.search_enrichment_service
    }

    pub fn embedding_cache(&self) -> &Arc<RwLock<Option<Arc<dyn EmbeddingPort>>>> {
        &self.search.embedding_cache
    }

    pub fn vector_search(&self) -> &Arc<dyn VectorSearchPort> {
        &self.search.vector_search
    }

    pub fn document_repo(&self) -> &Arc<dyn DocumentRepository> {
        &self.search.document_repo
    }
}


// ==============================================================================
// 3. IndexingModule - Document Ingestion and Processing
// ==============================================================================

/// Indexing module — hollow composition root over the indexing, web, and
/// batch feature slices. Shares `vector_search` with [`SearchModule`] so
/// index-time writes reach the same USearch instance queried at search time.
#[derive(Clone)]
pub struct IndexingModule {
    indexing: crate::features::indexing::di::IndexingDi,
    web: crate::features::web::di::WebDi,
    batch: crate::features::batch::di::BatchDi,
}

impl IndexingModule {
    /// Build IndexingModule by composing indexing + web + batch features.
    pub async fn new(
        db_pool: SqlitePool,
        core: Arc<CoreModule>,
        embedding_cache: Arc<RwLock<Option<Arc<dyn EmbeddingPort>>>>,
        vector_search: Arc<dyn VectorSearchPort>,
    ) -> crate::shared::error::Result<Self> {
        let indexing = crate::features::indexing::di::build(
            db_pool.clone(),
            embedding_cache.clone(),
            vector_search,
        )?;
        let model_dir = core.data_dir().join("models");
        let web = crate::features::web::di::build(db_pool, &model_dir, embedding_cache)?;
        let batch = crate::features::batch::di::build(
            indexing.batch_job_repo.clone(),
            indexing.index_file_use_case.clone(),
            web.ingest_web_url_use_case.clone(),
            web.web_ingestion_service.clone(),
            indexing.uow_factory.clone(),
        );

        Ok(Self { indexing, web, batch })
    }

    // Indexing use case getters
    pub fn index_file_use_case(&self) -> &Arc<IndexFileUseCase> {
        &self.indexing.index_file_use_case
    }

    pub fn index_directory_use_case(&self) -> &Arc<IndexDirectoryUseCase> {
        &self.indexing.index_directory_use_case
    }

    pub fn reindex_document_use_case(&self) -> &Arc<ReindexDocumentUseCase> {
        &self.indexing.reindex_document_use_case
    }

    pub fn delete_document_use_case(&self) -> &Arc<DeleteDocumentUseCase> {
        &self.indexing.delete_document_use_case
    }

    pub fn rename_document_use_case(&self) -> &Arc<RenameDocumentUseCase> {
        &self.indexing.rename_document_use_case
    }

    // Web use case getters
    pub fn ingest_web_url_use_case(&self) -> &Arc<IngestWebUrlUseCase> {
        &self.web.ingest_web_url_use_case
    }

    pub fn get_url_preview_use_case(&self) -> &Arc<GetUrlPreviewUseCase> {
        &self.web.get_url_preview_use_case
    }

    pub fn clean_article_content_use_case(&self) -> &Arc<CleanArticleContentUseCase> {
        &self.web.clean_article_content_use_case
    }

    // Batch use case getters
    pub fn start_batch_file_import_use_case(&self) -> &Arc<StartBatchFileImportUseCase> {
        &self.batch.start_batch_file_import_use_case
    }

    pub fn get_batch_file_status_use_case(&self) -> &Arc<GetBatchFileStatusUseCase> {
        &self.batch.get_batch_file_status_use_case
    }

    pub fn start_batch_url_import_use_case(&self) -> &Arc<StartBatchUrlImportUseCase> {
        &self.batch.start_batch_url_import_use_case
    }

    pub fn get_batch_job_status_use_case(&self) -> &Arc<GetBatchJobStatusUseCase> {
        &self.batch.get_batch_job_status_use_case
    }

    pub fn cancel_batch_job_use_case(&self) -> &Arc<CancelBatchJobUseCase> {
        &self.batch.cancel_batch_job_use_case
    }

    pub fn list_batch_jobs_use_case(&self) -> &Arc<ListBatchJobsUseCase> {
        &self.batch.list_batch_jobs_use_case
    }

    pub fn delete_batch_job_use_case(&self) -> &Arc<DeleteBatchJobUseCase> {
        &self.batch.delete_batch_job_use_case
    }

    pub fn retry_failed_items_use_case(&self) -> &Arc<RetryFailedItemsUseCase> {
        &self.batch.retry_failed_items_use_case
    }

    // Service getters
    pub fn indexing_state(&self) -> &Arc<crate::infrastructure::indexing::IndexingState> {
        &self.indexing.indexing_state
    }

    pub fn indexing_service(&self) -> &Arc<dyn IndexingServiceTrait> {
        &self.indexing.indexing_service
    }

    pub fn web_ingestion_service(&self) -> &Arc<dyn WebIngestionServiceTrait> {
        &self.web.web_ingestion_service
    }

    pub fn web_capture_service(&self) -> &Arc<dyn WebCaptureServiceTrait> {
        &self.web.web_capture_service
    }

    pub fn article_extractor_service(&self) -> &Arc<dyn ArticleExtractorServiceTrait> {
        &self.web.article_extractor_service
    }

    pub fn web_archive(&self) -> &Arc<dyn WebArchiveServiceTrait> {
        &self.web.web_archive
    }

    pub fn batch_file_import_service(&self) -> &Arc<dyn BatchFileImportServiceTrait> {
        &self.batch.batch_file_import_service
    }

    pub fn batch_url_import_service(&self) -> &Arc<dyn BatchUrlImportServiceTrait> {
        &self.batch.batch_url_import_service
    }

    pub fn file_storage(&self) -> &Arc<dyn FileStoragePort> {
        &self.indexing.file_storage
    }

    pub fn chunk_repository(&self) -> &Arc<dyn ChunkRepositoryPort> {
        &self.indexing.chunk_repo
    }

    pub fn batch_job_repo(&self) -> &Arc<dyn BatchJobRepositoryPort> {
        &self.indexing.batch_job_repo
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
    model_catalog_cache: Arc<crate::features::model_management::cache_adapter::ModelCacheAdapter>,
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
        use crate::features::model_management::huggingface_adapter::HuggingFaceAdapter;
        use crate::features::model_management::cache_adapter::ModelCacheAdapter;

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
        use crate::infrastructure::services::traits::ContextManagerTrait;
        use crate::features::qa::QAEngineTrait;

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
    ) -> &Arc<crate::features::model_management::cache_adapter::ModelCacheAdapter> {
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
    tags: crate::features::tags::di::TagsDi,
    favorites: crate::features::favorites::di::FavoritesDi,
    recent: crate::features::recent::di::RecentDi,
    mentions: crate::features::mentions::di::MentionsDi,
}

impl LibraryModule {
    /// Build LibraryModule by composing feature-local DI builders.
    pub async fn new(
        db_pool: SqlitePool,
        _core: Arc<CoreModule>,
    ) -> crate::shared::error::Result<Self> {
        Ok(Self {
            tags: crate::features::tags::di::build(db_pool.clone()),
            favorites: crate::features::favorites::di::build(db_pool.clone()),
            recent: crate::features::recent::di::build(db_pool.clone()),
            mentions: crate::features::mentions::di::build(db_pool),
        })
    }

    // Tag use case getters
    pub fn create_tag_use_case(&self) -> &Arc<CreateTagUseCase> {
        &self.tags.create_tag_use_case
    }

    pub fn update_tag_use_case(&self) -> &Arc<UpdateTagUseCase> {
        &self.tags.update_tag_use_case
    }

    pub fn delete_tag_use_case(&self) -> &Arc<DeleteTagUseCase> {
        &self.tags.delete_tag_use_case
    }

    pub fn remove_tag_from_document_use_case(&self) -> &Arc<RemoveTagFromDocumentUseCase> {
        &self.tags.remove_tag_from_document_use_case
    }

    pub fn get_tags_use_case(&self) -> &Arc<GetTagsUseCase> {
        &self.tags.get_tags_use_case
    }

    pub fn apply_tags_use_case(&self) -> &Arc<ApplyTagsUseCase> {
        &self.tags.apply_tags_use_case
    }

    pub fn search_by_tag_use_case(&self) -> &Arc<SearchByTagUseCase> {
        &self.tags.search_by_tag_use_case
    }

    // Favorites use case getters
    pub fn add_favorite_use_case(&self) -> &Arc<AddFavoriteUseCase> {
        &self.favorites.add_favorite_use_case
    }

    pub fn remove_favorite_use_case(&self) -> &Arc<RemoveFavoriteUseCase> {
        &self.favorites.remove_favorite_use_case
    }

    pub fn list_favorites_use_case(&self) -> &Arc<ListFavoritesUseCase> {
        &self.favorites.list_favorites_use_case
    }

    pub fn is_favorite_use_case(&self) -> &Arc<IsFavoriteUseCase> {
        &self.favorites.is_favorite_use_case
    }

    // Recent use case getters
    pub fn track_access_use_case(&self) -> &Arc<TrackAccessUseCase> {
        &self.recent.track_access_use_case
    }

    pub fn get_recent_documents_use_case(&self) -> &Arc<GetRecentDocumentsUseCase> {
        &self.recent.get_recent_documents_use_case
    }

    pub fn clear_recent_history_use_case(&self) -> &Arc<ClearRecentHistoryUseCase> {
        &self.recent.clear_recent_history_use_case
    }

    // Mentions use case getters
    pub fn extract_mentions_use_case(&self) -> &Arc<ExtractMentionsUseCase> {
        &self.mentions.extract_mentions_use_case
    }

    pub fn search_mentions_use_case(&self) -> &Arc<SearchMentionsUseCase> {
        &self.mentions.search_mentions_use_case
    }

    pub fn get_backlinks_use_case(&self) -> &Arc<GetBacklinksUseCase> {
        &self.mentions.get_backlinks_use_case
    }

    pub fn get_mentions_by_type_use_case(&self) -> &Arc<GetMentionsByTypeUseCase> {
        &self.mentions.get_mentions_by_type_use_case
    }

    pub fn get_mentions_for_document_use_case(&self) -> &Arc<GetMentionsForDocumentUseCase> {
        &self.mentions.get_mentions_for_document_use_case
    }

    pub fn favorites_repo(&self) -> &Arc<dyn FavoritesRepositoryPort> {
        &self.favorites.favorites_repo
    }

    pub fn recent_docs_repo(&self) -> &Arc<dyn RecentDocumentsRepositoryPort> {
        &self.recent.recent_docs_repo
    }

    pub fn create_mention_use_case(&self) -> &Arc<CreateMentionUseCase> {
        &self.mentions.create_mention_use_case
    }

    pub fn update_mention_use_case(&self) -> &Arc<UpdateMentionUseCase> {
        &self.mentions.update_mention_use_case
    }

    pub fn delete_mention_use_case(&self) -> &Arc<DeleteMentionUseCase> {
        &self.mentions.delete_mention_use_case
    }

    // Service getters
    pub fn tag_service(&self) -> &Arc<dyn TagServiceTrait> {
        &self.tags.tag_service
    }

    // Repository getters needed by Container
    pub fn mention_repo(&self) -> &Arc<dyn MentionRepositoryPort> {
        &self.mentions.mention_repo
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
    file: crate::features::file::di::FileDi,
    extraction: crate::features::extraction::di::ExtractionDi,
}

impl FileOpsModule {
    /// Build FileOpsModule by composing the file + extraction feature builders.
    pub async fn new(
        db_pool: SqlitePool,
        core: Arc<CoreModule>,
    ) -> crate::shared::error::Result<Self> {
        use crate::infrastructure::file_system::FileSystemAdapter;
        use crate::infrastructure::file_system::file_storage::SecureFileStorage;
        use crate::infrastructure::persistence::repositories::DocumentRepositoryImpl;
        use crate::features::tags::service::TagService;

        let file_system = Arc::new(FileSystemAdapter::new()) as Arc<dyn FileSystemPort>;
        let file_storage = Arc::new(SecureFileStorage::new()) as Arc<dyn FileStoragePort>;
        let file_access_config = Arc::clone(core.file_access_config());
        let vault_path = core
            .data_dir()
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| core.data_dir().clone());

        let document_repo =
            Arc::new(DocumentRepositoryImpl::new(db_pool.clone())) as Arc<dyn DocumentRepository>;
        let tag_service = Arc::new(TagService::new(db_pool)) as Arc<dyn TagServiceTrait>;

        let file = crate::features::file::di::build(
            document_repo.clone(),
            file_system,
            file_storage,
            tag_service,
            file_access_config,
            vault_path,
        );
        let extraction = crate::features::extraction::di::build(
            document_repo as Arc<dyn RepositoryPort<crate::domain::entities::Document>>,
        );

        Ok(Self { file, extraction })
    }

    // File operation use case getters
    pub fn open_file_use_case(&self) -> &Arc<OpenFileUseCase> {
        &self.file.open_file_use_case
    }

    pub fn open_file_by_id_use_case(&self) -> &Arc<OpenFileByIdUseCase> {
        &self.file.open_file_by_id_use_case
    }

    pub fn get_file_path_by_id_use_case(&self) -> &Arc<GetFilePathByIdUseCase> {
        &self.file.get_file_path_by_id_use_case
    }

    pub fn show_in_folder_use_case(&self) -> &Arc<ShowInFolderUseCase> {
        &self.file.show_in_folder_use_case
    }

    pub fn get_file_metadata_use_case(&self) -> &Arc<GetFileMetadataUseCase> {
        &self.file.get_file_metadata_use_case
    }

    pub fn read_file_content_use_case(&self) -> &Arc<ReadFileContentUseCase> {
        &self.file.read_file_content_use_case
    }

    pub fn read_file_bytes_use_case(&self) -> &Arc<ReadFileBytesUseCase> {
        &self.file.read_file_bytes_use_case
    }

    pub fn update_file_metadata_use_case(&self) -> &Arc<UpdateFileMetadataUseCase> {
        &self.file.update_file_metadata_use_case
    }

    // Extraction use case getters
    pub fn parse_wikilinks_use_case(&self) -> &Arc<ParseWikilinksUseCase> {
        &self.extraction.parse_wikilinks_use_case
    }

    pub fn extract_document_title_use_case(&self) -> &Arc<ExtractDocumentTitleUseCase> {
        &self.extraction.extract_document_title_use_case
    }

    pub fn resolve_wikilink_use_case(&self) -> &Arc<ResolveWikilinkUseCase> {
        &self.extraction.resolve_wikilink_use_case
    }

    pub fn extract_and_resolve_links_use_case(&self) -> &Arc<ExtractAndResolveLinksUseCase> {
        &self.extraction.extract_and_resolve_links_use_case
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
    settings: crate::features::settings::di::SettingsDi,
    cache: crate::features::cache::di::CacheDi,
    backup: crate::features::backup::di::BackupDi,
    updates: crate::features::updates::di::UpdatesDi,
    metrics: crate::features::metrics::di::MetricsDi,
    health: crate::features::health::di::HealthDi,
    stats: crate::features::stats::di::StatsDi,
    initialization: crate::features::initialization::di::InitializationDi,

    // System-info adapter stays here — it's a shared infra concern
    // with no dedicated feature slice.
    system_info: Arc<dyn SystemInfoPort>,
}

impl SystemModule {
    /// Build SystemModule by composing system-level feature builders.
    pub async fn new(
        db_pool: SqlitePool,
        core: Arc<CoreModule>,
    ) -> crate::shared::error::Result<Self> {
        use crate::infrastructure::system_info_adapter::SystemInfoAdapter;

        let settings_path = core.data_dir().join("settings.json");
        let settings = crate::features::settings::di::build(&settings_path).await?;

        let db_path = core.data_dir().join("vault.db");
        let backup = crate::features::backup::di::build(
            db_pool.clone(),
            db_path,
            settings.settings_repo.clone(),
        );

        let models_path = core.data_dir().join("models");
        let model_manager = Arc::new(ModelManager::new(models_path)?) as Arc<dyn ModelManagerTrait>;
        let initialization =
            crate::features::initialization::di::build(db_pool.clone(), model_manager);

        Ok(Self {
            settings,
            cache: crate::features::cache::di::build(),
            backup,
            updates: crate::features::updates::di::build(),
            metrics: crate::features::metrics::di::build(),
            health: crate::features::health::di::build(db_pool.clone()),
            stats: crate::features::stats::di::build(db_pool),
            initialization,
            system_info: Arc::new(SystemInfoAdapter::new()) as Arc<dyn SystemInfoPort>,
        })
    }

    // Health use case getters
    pub fn health_check_use_case(&self) -> &Arc<HealthCheckUseCase> {
        &self.health.health_check_use_case
    }

    pub fn initialize_database_use_case(&self) -> &Arc<InitializeDatabaseUseCase> {
        &self.initialization.initialize_database_use_case
    }

    pub fn initialize_models_use_case(&self) -> &Arc<InitializeModelsUseCase> {
        &self.initialization.initialize_models_use_case
    }

    // Settings use case getters
    pub fn get_settings_use_case(&self) -> &Arc<GetSettingsUseCase> {
        &self.settings.get_settings_use_case
    }

    pub fn update_settings_use_case(&self) -> &Arc<UpdateSettingsUseCase> {
        &self.settings.update_settings_use_case
    }

    pub fn reset_settings_use_case(&self) -> &Arc<ResetSettingsUseCase> {
        &self.settings.reset_settings_use_case
    }

    pub fn export_settings_use_case(&self) -> &Arc<ExportSettingsUseCase> {
        &self.settings.export_settings_use_case
    }

    pub fn import_settings_use_case(&self) -> &Arc<ImportSettingsUseCase> {
        &self.settings.import_settings_use_case
    }

    pub fn validate_settings_use_case(&self) -> &Arc<ValidateSettingsUseCase> {
        &self.settings.validate_settings_use_case
    }

    // Cache use case getters
    pub fn clear_cache_use_case(&self) -> &Arc<ClearCacheUseCase> {
        &self.cache.clear_cache_use_case
    }

    pub fn get_cache_size_use_case(&self) -> &Arc<GetCacheSizeUseCase> {
        &self.cache.get_cache_size_use_case
    }

    pub fn get_cache_stats_use_case(&self) -> &Arc<GetCacheStatsUseCase> {
        &self.cache.get_cache_stats_use_case
    }

    // Backup use case getters
    pub fn create_backup_use_case(&self) -> &Arc<CreateBackupUseCase> {
        &self.backup.create_backup_use_case
    }

    pub fn restore_backup_use_case(&self) -> &Arc<RestoreBackupUseCase> {
        &self.backup.restore_backup_use_case
    }

    pub fn list_backups_use_case(&self) -> &Arc<ListBackupsUseCase> {
        &self.backup.list_backups_use_case
    }

    pub fn start_auto_backup_use_case(&self) -> &Arc<StartAutoBackupUseCase> {
        &self.backup.start_auto_backup_use_case
    }

    pub fn stop_auto_backup_use_case(&self) -> &Arc<StopAutoBackupUseCase> {
        &self.backup.stop_auto_backup_use_case
    }

    pub fn startup_auto_backup_use_case(&self) -> &Arc<StartupAutoBackupUseCase> {
        &self.backup.startup_auto_backup_use_case
    }

    pub fn backup_scheduler(&self) -> &Arc<crate::features::backup::scheduler::BackupScheduler> {
        &self.backup.backup_scheduler
    }

    // Updates use case getters
    pub fn check_for_updates_use_case(&self) -> &Arc<CheckForUpdatesUseCase> {
        &self.updates.check_for_updates_use_case
    }

    pub fn get_current_version_use_case(&self) -> &Arc<GetCurrentVersionUseCase> {
        &self.updates.get_current_version_use_case
    }

    // Metrics use case getters
    pub fn get_metrics_use_case(&self) -> &Arc<GetMetricsUseCase> {
        &self.metrics.get_metrics_use_case
    }

    // Stats use case getters
    pub fn get_system_stats_use_case(&self) -> &Arc<GetSystemStatsUseCase> {
        &self.stats.get_system_stats_use_case
    }

    // Adapter getters needed by Container
    pub fn system_info(&self) -> &Arc<dyn SystemInfoPort> {
        &self.system_info
    }

    pub fn metrics_service(&self) -> &Arc<Metrics> {
        &self.metrics.metrics_service
    }
}
