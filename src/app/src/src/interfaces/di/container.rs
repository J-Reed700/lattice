//! DDD-Aligned Dependency Injection Container
//!
//! This container replaces the old AppState god object with proper DDD layering:
//!
//! Layers (inside-out):
//! 1. Domain - Pure business logic, no dependencies
//! 2. Application - Use cases, depends on ports (abstractions)
//! 3. Infrastructure - Concrete implementations of ports
//! 4. Interfaces - Command handlers (this layer)
//!
//! The Container:
//! - Builds infrastructure adapters (implementations)
//! - Wires use cases with port dependencies
//! - Provides getters for commands to access use cases
//! - Manages shared resources (DB pool, security context)

use crate::infrastructure::persistence::helpers::query_indexed_directories;
use crate::infrastructure::security::{FileAccessConfig, SecurityContext};
use crate::shared::error::{AppError, Result};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Instant;

type NamedLlmCache = Arc<RwLock<Option<(String, Arc<dyn LLMPort>)>>>;

// Application Use Cases - Core
use crate::features::health::use_cases::HealthCheckUseCase;
use crate::features::indexing::use_cases::{
    DeleteDocumentUseCase, IndexDirectoryUseCase, IndexFileUseCase, ReindexDocumentUseCase,
    RenameDocumentUseCase,
};
use crate::features::initialization::use_cases::{
    InitializeDatabaseUseCase, InitializeModelsUseCase,
};
use crate::features::qa::use_cases::AskQuestionUseCase;
use crate::features::search::use_cases::{HybridSearchUseCase, SemanticSearchUseCase};
use crate::features::stats::use_cases::{GetCorpusShapeUseCase, GetSystemStatsUseCase};
use crate::features::tags::use_cases::{
    ApplyTagsUseCase, AutoTagAllDocumentsUseCase, CreateTagUseCase, DeleteTagUseCase,
    GenerateTagsUseCase, GetTagsUseCase, RemoveTagFromDocumentUseCase, SearchByTagUseCase,
    UpdateTagUseCase,
};

// Application Use Cases - Conversation
use crate::features::conversation::use_cases::CreateConversationUseCase;

// Application Use Cases - Settings
use crate::features::settings::use_cases::{
    ExportSettingsUseCase, GetSettingsUseCase, ImportSettingsUseCase, ResetSettingsUseCase,
    UpdateSettingsUseCase, ValidateSettingsUseCase,
};

// Application Use Cases - File Operations
use crate::features::file::use_cases::{
    GetFileMetadataUseCase, GetFilePathByIdUseCase, ListCitingConversationsUseCase,
    OpenFileByIdUseCase, OpenFileUseCase, ReadFileBytesUseCase, ReadFileContentUseCase,
    ShowInFolderUseCase, UpdateFileMetadataUseCase,
};

// (Favorites + Recent: no use cases — Tauri commands route through
// raw sqlx in their respective commands.rs files.)

// Application Use Cases - Mentions
use crate::features::mentions::use_cases::{
    CreateMentionUseCase, DeleteMentionUseCase, ExtractMentionsUseCase, GetBacklinksUseCase,
    GetMentionsByTypeUseCase, GetMentionsForDocumentUseCase, SearchMentionsUseCase,
};

// Application Use Cases - Extraction
use crate::features::extraction::use_cases::{
    ExtractAndResolveLinksUseCase, ExtractDocumentTitleUseCase, ParseWikilinksUseCase,
    ResolveWikilinkUseCase,
};

// Application Use Cases - Credentials
use crate::features::credentials::use_cases::{
    DeleteApiKeyUseCase, GetApiKeyUseCase, SetApiKeyUseCase, SetCustomEndpointUseCase,
};

// (Cache: no use cases — commands operate on the global QUERY_CACHE
// singleton directly.)

// Application Use Cases - Backup
use crate::features::backup::use_cases::{
    CreateBackupUseCase, RestoreBackupUseCase, StartAutoBackupUseCase, StartupAutoBackupUseCase,
    StopAutoBackupUseCase,
};

// Application Use Cases - Updates
use crate::features::updates::use_cases::{CheckForUpdatesUseCase, GetCurrentVersionUseCase};

// Application Use Cases - Metrics
use crate::features::metrics::use_cases::GetMetricsUseCase;

// Application Use Cases - LLM
use crate::features::llm::use_cases::{
    CheckModelDownloadedUseCase, DeleteModelUseCase, DownloadModelUseCase,
    GetAvailableModelsUseCase, GetBestModelUseCase, GetModelPathUseCase,
    GetRecommendedModelsUseCase, GetSystemCapabilitiesUseCase, ListDownloadedModelsUseCase,
};
use crate::features::settings::dto::LLMProvider;

// Application Use Cases - Web
use crate::features::web::use_cases::{GetUrlPreviewUseCase, IngestWebUrlUseCase};

// Application Use Cases - Batch
use crate::features::batch::use_cases::{
    CancelBatchJobUseCase, DeleteBatchJobUseCase, GetBatchJobStatusUseCase, ListBatchJobsUseCase,
    RetryFailedItemsUseCase, StartBatchFileImportUseCase, StartBatchUrlImportUseCase,
};

// Application Ports (abstractions)
use crate::application::ports::batch_job_repository_port::BatchJobRepositoryPort;
use crate::application::ports::model_storage::ModelStoragePort;
use crate::application::ports::{
    BackupPort, ChunkRepositoryPort, ContentAddressedStoragePort, ContentExtractionPort,
    CredentialsPort, DocumentRepository, DocumentRepositoryPort, EmbeddingPort,
    EmbeddingRepositoryPort, FavoritesRepositoryPort, FileStoragePort, FileSystemPort, LLMPort,
    MentionRepositoryPort, MetricsPort, ModelCatalogPort, RecentDocumentsRepositoryPort,
    RepositoryPort, SettingsRepositoryPort, SettingsSideEffectsPort, SystemInfoPort,
    TextSearchPort, UpdateCheckerPort, VectorSearchPort,
};
use crate::domain::ports::file_access::ChecksumService;
use crate::llm::LLMClient;

// Domain Entities and Aggregates (for type annotations)
use crate::domain::entities::chunk::Chunk;
use crate::domain::entities::document::Document;
use crate::domain::entities::Document as DocumentEntity;
use crate::features::tags::entity::Tag as TagEntity;

// Infrastructure Implementations - ML & Search
use crate::features::embedding::candle_service::CandleEmbeddingService;
use crate::infrastructure::file_system::file_storage::SecureFileStorage;
use crate::infrastructure::llm::noop_client::NoOpLLMClient;
use crate::infrastructure::llm::ollama_client::OllamaClient;
use crate::infrastructure::search::text_search::SqliteTextSearch;
// USearchVectorIndex is used directly via modules.rs — no direct import needed here
use crate::infrastructure::storage::ContentAddressedStorage;

// Infrastructure Implementations - Repositories
use crate::features::favorites::repository::FavoritesRepository;
use crate::features::recent::repository::RecentDocumentsRepository;
use crate::infrastructure::persistence::repositories::{
    unit_of_work::SqliteUnitOfWorkFactory, BatchJobRepository, ChunkRepositoryImpl,
    DocumentRepositoryImpl, EmbeddingRepository, MentionRepository, SettingsRepository,
    TagRepositoryImpl,
};

// Infrastructure Implementations - Adapters
use crate::features::backup::adapter::BackupAdapter;
use crate::features::credentials::adapter::CredentialsAdapter;
use crate::features::metrics::adapter::MetricsAdapter;
use crate::features::model_management::cache_adapter::ModelCacheAdapter;
use crate::features::model_management::huggingface_adapter::HuggingFaceAdapter;
use crate::features::updates::adapter::UpdateCheckerAdapter;
use crate::infrastructure::adapters::{ContentExtractionAdapter, TokioChecksumAdapter};
use crate::infrastructure::file_system::file_system_adapter::FileSystemAdapter;
use crate::infrastructure::llm::model_storage_adapter::FilesystemModelStorage;
use crate::infrastructure::system_info_adapter::SystemInfoAdapter;

// Service Implementations
use crate::infrastructure::event_bus::EventBus;
use crate::infrastructure::indexing::IndexingService;
use crate::infrastructure::observability::metrics::Metrics;
use crate::infrastructure::qa::engine::QAEngine;
use crate::infrastructure::search::bm25::BM25Search;
use crate::infrastructure::search::hybrid::HybridSearchService;
// VectorSearchService removed — USearchVectorIndex implements SearchServiceTrait directly
use crate::features::batch::BatchFileImportServiceTrait;
use crate::features::batch::BatchUrlImportServiceTrait;
use crate::features::cache::llm_cache::LlmCache;
use crate::features::conversation::ConversationServiceTrait;
use crate::features::embedding::service::DynamicEmbeddingService;
use crate::features::embedding::EmbeddingServiceTrait;
use crate::features::function_calling::{FunctionExecutorTrait, FunctionRegistryTrait};
use crate::features::indexing::IndexingServiceTrait;
use crate::features::qa::conversational_service::ConversationalQAService;
use crate::features::qa::ConversationalQAServiceTrait;
use crate::features::qa::QAEngineTrait;
use crate::features::search::{BM25SearchTrait, HybridSearchTrait, SearchServiceTrait};
use crate::features::tags::service_impl::TagServiceImpl;
use crate::features::tags::TagServiceTrait;
use crate::features::web::WebArchiveServiceTrait;
use crate::features::web::WebCaptureServiceTrait;
use crate::features::web::WebIngestionServiceTrait;
use crate::infrastructure::services::context_manager::ContextManager;
use crate::infrastructure::services::search_enrichment_service::SearchEnrichmentService;
use crate::infrastructure::services::traits::ArticleExtractorServiceTrait;
use crate::infrastructure::services::traits::ContextManagerTrait;
use crate::infrastructure::services::traits::SearchEnrichmentServiceTrait;
use crate::infrastructure::services::ArticleExtractorService;
use crate::infrastructure::services::BatchFileImportService;
use crate::infrastructure::services::BatchUrlImportService;
use crate::infrastructure::services::ConversationService;
use crate::infrastructure::services::WebArchiveService;
use crate::infrastructure::services::WebCaptureService;
use crate::infrastructure::services::WebIngestionService;
use crate::infrastructure::services::{
    init_function_registry, register_custom_query_tools, FunctionExecutor, WebService,
};

/// HOLLOW CONTAINER - Modular Architecture (2026-01-21)
///
/// This is the "Strangler Fig" pattern - Container now holds 7 domain modules
/// instead of 102 individual fields. This fixes stack overflow while maintaining
/// backward compatibility via delegate methods.
///
/// **Old**: 102 fields (God Object) → Stack overflow on 2MB threads
/// **New**: 7 module fields → Each module is small and manageable
///
/// Commands can use either:
/// - `State<'_, Container>` for legacy compatibility (delegates to modules)
/// - `State<'_, SearchModule>` for direct module access (modern, less stack pressure)
#[derive(Clone)]
pub struct Container {
    /// Core infrastructure (DB, security, runtime, credentials)
    pub core: Arc<super::modules::CoreModule>,

    /// System operations (settings, health, backup, cache, updates, metrics, stats)
    pub system: Arc<super::modules::SystemModule>,

    /// File system operations (open, metadata, wikilinks, extraction)
    pub file_ops: Arc<super::modules::FileOpsModule>,

    /// Document management (tags, favorites, recent, mentions)
    pub library: Arc<super::modules::LibraryModule>,

    /// AI operations (LLM, conversations, Q&A, AI-powered tags)
    pub ai: Arc<super::modules::AIModule>,

    /// Document ingestion (indexing, web scraping, batch imports)
    pub indexing: Arc<super::modules::IndexingModule>,

    /// Search operations (semantic, hybrid, file, recency)
    pub search: Arc<super::modules::SearchModule>,

    /// Function calling registry for LLM tools
    function_registry: Arc<dyn FunctionRegistryTrait>,

    /// Function executor for LLM tools
    function_executor: Arc<dyn FunctionExecutorTrait>,

    /// Router LLM cache (optional smaller routing model)
    router_llm_cache: NamedLlmCache,

    /// Utility LLM cache (HyDE expansion, intent routing, follow-up)
    utility_llm_cache: NamedLlmCache,

    /// Per-role async load locks coalesce cache misses. Model construction may
    /// allocate multiple gigabytes or spawn a sidecar, so duplicate loads are
    /// not an acceptable implementation of double-checked locking.
    llm_load_lock: Arc<tokio::sync::Mutex<()>>,
    router_llm_load_lock: Arc<tokio::sync::Mutex<()>>,
    utility_llm_load_lock: Arc<tokio::sync::Mutex<()>>,
    embedding_load_lock: Arc<tokio::sync::Mutex<()>>,

    /// All vault writes flow through a single mpsc-fed worker; see
    /// `features::vault::writeback`.
    vault_writer: crate::features::vault::writeback::VaultWriterHandle,

    /// Shared by writer + watcher for loop suppression.
    vault_write_suppression: crate::features::vault::watcher::WriteSuppressionRegistry,

    /// Cooldown timestamp for embedding load failures.
    /// When a `ModelLoadFailed` error occurs, we record the time so that
    /// subsequent calls within the cooldown window return the same error
    /// immediately instead of retrying (and spamming logs).
    embedding_error_cooldown: Arc<parking_lot::RwLock<Option<(Instant, AppError)>>>,

    /// Settings side-effects port (audit P0-3 fix). Built post-AI-module
    /// so it has access to the LLM cache + router LLM cache + function
    /// executor. Injected into freshly-constructed
    /// `UpdateSettingsUseCase` / `ResetSettingsUseCase` instances on
    /// every `Container::*_settings_use_case()` call so internal
    /// callers (tests, watch-folder helpers, future migration scripts)
    /// trigger cache invalidation just like the Tauri command path
    /// did manually before.
    settings_side_effects: Arc<dyn SettingsSideEffectsPort>,

    /// Tauri AppHandle, populated at app boot via
    /// `with_app_handle()`. Required by the LLM factory's sidecar
    /// dispatch — `tauri-plugin-shell` needs it to spawn the bundled
    /// `llama-server` child process. `None` only in test fixtures
    /// (MockAppContainer covers most test paths); LLM-load-time
    /// `InvalidConfig` errors fire if a real path tries to use the
    /// LLM with no handle attached.
    ///
    /// This field is the audit P1-2 ("Container god-object") tradeoff
    /// made deliberately: the sidecar architecture genuinely
    /// requires the AppHandle to reach the LLM factory, and threading
    /// it through every call would add a parameter to dozens of
    /// methods. The long-term cleanup is to split Container into a
    /// wiring layer + a runtime-state layer (audit P1-2).
    app_handle: Option<tauri::AppHandle>,
}

// ============================================================================
// SettingsSideEffectsPort impl — audit P0-3 fix
// ============================================================================

/// Container-bound implementation of `SettingsSideEffectsPort`. Holds
/// references to the same primitive caches the legacy
/// `Container::invalidate_*` methods touched, so a settings update or
/// reset triggered through any code path (Tauri command, internal
/// helper, test) invalidates the LLM cache, router LLM cache, and
/// custom-tool runtime configuration.
///
/// Each field is the same `Arc` that lives on Container — they share
/// the same underlying `RwLock`, so this impl observes the same state
/// the rest of Container does.
struct ContainerSettingsSideEffects {
    /// LLM client cache. Same `Arc` as `Container::ai.llm_cache()`.
    llm_cache: Arc<RwLock<Option<Arc<dyn LLMPort>>>>,

    /// Router LLM cache. Same `Arc` as `Container::router_llm_cache`.
    router_llm_cache: NamedLlmCache,

    /// Function executor for refreshing custom-tool runtime config.
    function_executor: Arc<dyn FunctionExecutorTrait>,

    /// Settings repository for re-reading custom_tools after a write.
    settings_repository: Arc<dyn SettingsRepositoryPort>,

    /// Used by the vault backfill side effect.
    db_pool: sqlx::SqlitePool,

    /// Backfill submits through this so it shares the FIFO queue with
    /// per-note writes.
    vault_writer: crate::features::vault::writeback::VaultWriterHandle,
}

#[async_trait::async_trait]
impl SettingsSideEffectsPort for ContainerSettingsSideEffects {
    async fn on_settings_updated(
        &self,
        category: Option<crate::features::settings::dto::SettingsCategory>,
        hints: crate::application::ports::settings_side_effects_port::SettingsTransitionHints,
    ) {
        use crate::features::settings::dto::SettingsCategory;

        // Submit through the writer queue so backfill shares the FIFO
        // with per-note writes and can't race fresh edits.
        if hints.vault_just_enabled {
            match self.settings_repository.get_all().await {
                Ok(settings) => {
                    if let Some(vault_root) = crate::features::vault::writeback::resolve_vault_root(
                        &settings.vault.vault_path,
                    ) {
                        tracing::info!(
                            vault_root = %vault_root.display(),
                            "Vault enabled — enqueueing one-shot backfill"
                        );
                        self.vault_writer.submit(
                            crate::features::vault::writeback::VaultWriteJob::Backfill {
                                pool: self.db_pool.clone(),
                                vault_root,
                            },
                        );
                    } else {
                        tracing::warn!(
                            "Vault enabled but vault root could not be resolved — skipping backfill"
                        );
                    }
                }
                Err(e) => tracing::warn!(
                    error = %e,
                    "Vault enabled but settings re-read failed — skipping backfill"
                ),
            }
        }

        // Invalidate the LLM caches when LLM settings (or a global
        // / no-category mutation, which could touch anything) changed.
        // Other categories (Search, Indexing, Display, etc.) don't
        // affect the LLM cache — skipping the invalidation here means
        // a search-config change doesn't cause a needless model
        // reload on the next chat turn.
        let touched_llm = matches!(category, Some(SettingsCategory::Llm) | None);

        if touched_llm {
            {
                let mut cache = self.llm_cache.write().unwrap_or_else(|p| p.into_inner());
                if cache.is_some() {
                    tracing::info!("Settings update touched LLM category — invalidating LLM cache");
                    *cache = None;
                }
            }
            {
                let mut cache = self
                    .router_llm_cache
                    .write()
                    .unwrap_or_else(|p| p.into_inner());
                if cache.is_some() {
                    tracing::info!(
                        "Settings update touched LLM category — invalidating router LLM cache"
                    );
                    *cache = None;
                }
            }
            // Custom-tool runtime config refresh — re-read settings
            // and push the active custom_tools map into the function
            // executor. Best-effort: log but don't fail.
            match self.settings_repository.get_all().await {
                Ok(settings) => {
                    let custom_tool_map: std::collections::HashMap<
                        String,
                        crate::features::settings::dto::CustomToolSettingsDto,
                    > = settings
                        .llm
                        .custom_tools
                        .into_iter()
                        .filter(|tool| tool.enabled)
                        .map(|tool| (tool.name.clone(), tool))
                        .collect();
                    self.function_executor.set_custom_tools(custom_tool_map);
                }
                Err(error) => {
                    tracing::warn!(
                        "Failed to refresh custom tools after settings update; \
                         keeping previous configuration: {error}"
                    );
                }
            }
        }
    }
}

impl Container {
    // === SECURITY FIX (CWE-755): RwLock Poison Recovery Helpers ===

    /// Recover from RwLock poison error by extracting the inner value
    ///
    /// SECURITY: CWE-755 mitigation - Improper handling of exceptional conditions
    ///
    /// When a thread panics while holding an RwLock, Rust "poisons" the lock to
    /// prevent data corruption. However, in this application:
    /// - The cached values (Arc<dyn Port>) are immutable after creation
    /// - Panic during load is safe to recover from (worst case: reload service)
    /// - Alternative: Panic would crash the entire app (worse UX)
    ///
    /// Recovery strategy:
    /// 1. Log the poison error with ERROR level
    /// 2. Extract the inner value (still valid since Arc is immutable)
    /// 3. Continue execution with the cached value
    /// 4. Optionally fall back to CPU if value is None
    ///
    /// # Arguments
    ///
    /// * `lock_result` - Result from RwLock::read() or write()
    ///
    /// # Returns
    ///
    /// The guard value, recovering from poison if necessary
    fn recover_read_lock<T: Clone>(
        lock_result: std::sync::LockResult<std::sync::RwLockReadGuard<'_, T>>,
    ) -> T {
        match lock_result {
            Ok(guard) => guard.clone(),
            Err(poison_err) => {
                tracing::error!(
                    "RwLock poisoned during read - recovering with cached value. \
                     This indicates a panic occurred while the lock was held."
                );
                poison_err.into_inner().clone()
            }
        }
    }

    /// Recover from write lock poison error
    ///
    /// See recover_read_lock for security rationale.
    fn recover_write_lock<T>(
        lock_result: std::sync::LockResult<std::sync::RwLockWriteGuard<'_, T>>,
    ) -> std::sync::RwLockWriteGuard<'_, T> {
        match lock_result {
            Ok(guard) => guard,
            Err(poison_err) => {
                tracing::error!(
                    "RwLock poisoned during write - recovering. \
                     Lock will be cleared and write will proceed."
                );
                poison_err.into_inner()
            }
        }
    }

    /// Build Container with hierarchical module initialization
    ///
    /// This is the "Hollow Container" - it just orchestrates module creation.
    /// Each module builds its own dependencies, preventing stack overflow.
    ///
    /// **Stack Safety**: Each module's constructor allocates on its own stack frame,
    /// which is freed before the next module starts. This prevents the 2MB overflow.
    ///
    /// **Architecture**: Layer 0 (raw params) → Layer 1 (CoreModule) → Layer 2 (Features)
    pub async fn new(
        db_pool: SqlitePool,
        db_conn: Arc<crate::infrastructure::persistence::database::DatabaseConnection>,
        embedding_model_path: Option<String>,
        llm_endpoint: &str,
        llm_model: &str,
        data_dir: PathBuf,
    ) -> Result<Self> {
        // === Create Shared State Caches ===
        // These are populated asynchronously when models load
        let embedding_cache = Arc::new(RwLock::new(None));
        let llm_cache = Arc::new(RwLock::new(None));
        let router_llm_cache = Arc::new(RwLock::new(None));
        let utility_llm_cache = Arc::new(RwLock::new(None));
        let llm_load_lock = Arc::new(tokio::sync::Mutex::new(()));
        let router_llm_load_lock = Arc::new(tokio::sync::Mutex::new(()));
        let utility_llm_load_lock = Arc::new(tokio::sync::Mutex::new(()));
        let embedding_load_lock = Arc::new(tokio::sync::Mutex::new(()));
        tracing::info!("Building Container with modular architecture (Hollow Container Pattern)");

        // === Layer 1: Core Infrastructure ===
        // CoreModule builds shared singletons (security, runtime, credentials)
        // Stack frame for this construction is freed before Layer 2 begins
        let core = Arc::new(
            super::modules::CoreModule::new(db_pool.clone(), db_conn.clone(), data_dir.clone())
                .await?,
        );

        tracing::debug!("CoreModule initialized");

        // === Layer 2: Feature Modules ===
        // Each module builds its own dependencies. Stack frames are independent.

        let system =
            Arc::new(super::modules::SystemModule::new(db_pool.clone(), core.clone()).await?);
        tracing::debug!("SystemModule initialized");

        let search = Arc::new(
            super::modules::SearchModule::new(
                db_pool.clone(),
                core.clone(),
                embedding_cache.clone(),
            )
            .await?,
        );
        tracing::debug!("SearchModule initialized");

        let library =
            Arc::new(super::modules::LibraryModule::new(db_pool.clone(), core.clone()).await?);
        tracing::debug!("LibraryModule initialized");

        let file_ops =
            Arc::new(super::modules::FileOpsModule::new(db_pool.clone(), core.clone()).await?);
        tracing::debug!("FileOpsModule initialized");

        let ai = Arc::new(
            super::modules::AIModule::new(
                db_pool.clone(),
                core.clone(),
                llm_cache,
                llm_endpoint,
                llm_model,
            )
            .await?,
        );
        tracing::debug!("AIModule initialized");

        let indexing = Arc::new(
            super::modules::IndexingModule::new(
                db_pool,
                core.clone(),
                embedding_cache,
                search.vector_search().clone(),
            )
            .await?,
        );
        tracing::debug!("IndexingModule initialized");

        tracing::info!("Container fully initialized with 7 modules");

        // === Function Calling Infrastructure ===
        let function_registry = Arc::new(init_function_registry()?);
        let configured_custom_tools = match system.get_settings_use_case().execute().await {
            Ok(settings) => settings.llm.custom_tools,
            Err(e) => {
                tracing::warn!(
                    "Failed to load settings for custom tool registration; continuing with built-in tools only: {}",
                    e
                );
                Vec::new()
            }
        };

        if let Err(e) =
            register_custom_query_tools(function_registry.as_ref(), &configured_custom_tools)
        {
            tracing::warn!(
                "Failed to register one or more custom tools during startup: {}",
                e
            );
        }
        let custom_tool_map: HashMap<
            String,
            crate::features::settings::dto::CustomToolSettingsDto,
        > = configured_custom_tools
            .into_iter()
            .filter(|tool| tool.enabled)
            .map(|tool| (tool.name.clone(), tool))
            .collect();

        let embedding_service = Arc::new(DynamicEmbeddingService::new(
            search.embedding_cache().clone(),
        )) as Arc<dyn EmbeddingServiceTrait>;
        let web_service = Arc::new(WebService::new()?);

        let function_executor = Arc::new(FunctionExecutor::new_with_custom_tools(
            function_registry.clone(),
            embedding_service,
            search.search_service().clone(),
            search.bm25_search_service().clone(),
            search.hybrid_search_service().clone(),
            search.document_repo().clone(),
            indexing.chunk_repository().clone(),
            library.tag_service().clone(),
            library.favorites_repo().clone(),
            library.recent_docs_repo().clone(),
            indexing.file_storage().clone(),
            web_service,
            custom_tool_map,
        )) as Arc<dyn FunctionExecutorTrait>;

        let vault_write_suppression =
            crate::features::vault::watcher::WriteSuppressionRegistry::new();
        let vault_writer = crate::features::vault::writeback::start_vault_writer(
            Arc::clone(system.get_settings_use_case()),
            vault_write_suppression.clone(),
        );

        let settings_side_effects: Arc<dyn SettingsSideEffectsPort> =
            Arc::new(ContainerSettingsSideEffects {
                llm_cache: ai.llm_cache().clone(),
                router_llm_cache: router_llm_cache.clone(),
                function_executor: function_executor.clone(),
                settings_repository: system.settings_repo().clone(),
                db_pool: core.db_pool().clone(),
                vault_writer: vault_writer.clone(),
            });

        Ok(Self {
            core,
            system,
            file_ops,
            library,
            ai,
            indexing,
            search,
            function_registry,
            function_executor,
            router_llm_cache,
            utility_llm_cache,
            llm_load_lock,
            router_llm_load_lock,
            utility_llm_load_lock,
            embedding_load_lock,
            vault_writer,
            vault_write_suppression,
            embedding_error_cooldown: Arc::new(parking_lot::RwLock::new(None)),
            settings_side_effects,
            app_handle: None,
        })
    }

    pub fn vault_writer(&self) -> &crate::features::vault::writeback::VaultWriterHandle {
        &self.vault_writer
    }

    /// Belt-and-suspenders safety net for the fs watcher.
    /// No-op when vault is disabled or the watch toggle is off.
    pub async fn rescan_vault(&self) -> Result<crate::features::vault::watcher::RescanSummary> {
        let settings = self
            .system
            .get_settings_use_case()
            .execute()
            .await
            .map_err(|e| AppError::InvalidConfig(format!("Failed to load settings: {}", e)))?;
        if !settings.vault.enabled || !settings.vault.watch_external_changes {
            return Ok(crate::features::vault::watcher::RescanSummary {
                scanned: 0,
                imported: 0,
                deleted: 0,
            });
        }
        let vault_root =
            crate::features::vault::writeback::resolve_vault_root(&settings.vault.vault_path)
                .ok_or_else(|| {
                    AppError::InvalidConfig(
                        "Vault root could not be resolved (no home directory)".to_string(),
                    )
                })?;
        let app_handle = self.app_handle.clone().ok_or_else(|| {
            AppError::InvalidConfig(
                "AppHandle not installed — Container constructed without with_app_handle"
                    .to_string(),
            )
        })?;
        crate::features::vault::watcher::rescan_vault(
            self.core.db_pool().clone(),
            vault_root,
            self.vault_write_suppression.clone(),
            app_handle,
        )
        .await
        .map_err(AppError::Other)
    }

    /// Attach the Tauri `AppHandle` post-construction. Real callers chain
    /// `.with_app_handle(handle)`; tests skip it.
    pub fn with_app_handle(mut self, handle: tauri::AppHandle) -> Self {
        self.vault_writer.set_app_handle(handle.clone());
        // Watcher reads settings once and exits if disabled.
        crate::features::vault::watcher::start_vault_watcher(
            Arc::clone(self.system.get_settings_use_case()),
            self.core.db_pool().clone(),
            handle.clone(),
            self.vault_write_suppression.clone(),
        );

        self.app_handle = Some(handle);
        self
    }

    /// Returns the stored AppHandle, if any. Used by the LLM factory
    /// to populate `LLMConfig::Local.app_handle` when the sidecar
    /// feature is enabled.
    fn app_handle(&self) -> Option<tauri::AppHandle> {
        self.app_handle.clone()
    }

    // === Lazy LLM Loading ===

    /// Gets the LLM service, loading it lazily if needed.
    ///
    /// Uses double-check locking pattern to minimize lock contention:
    /// 1. Fast path: Check cache with read lock (no loading)
    /// 2. Slow path: Load outside any lock, then store with write lock
    ///
    /// Thread-safe: Multiple threads may load simultaneously, but only one
    /// result is cached (last writer wins, which is safe since all loads
    /// produce equivalent services).
    pub async fn get_or_load_llm(&self) -> Result<Arc<dyn LLMPort>> {
        // Delegate to AIModule's llm_cache (via getter)
        // FAST PATH: Check if already cached (read lock with poison recovery)
        {
            let cache = Self::recover_read_lock(self.ai.llm_cache().read());
            if let Some(service) = cache.as_ref() {
                return Ok(Arc::clone(service));
            }
        } // ← Read lock released before I/O

        let _load_guard = self.llm_load_lock.lock().await;
        {
            let cache = Self::recover_read_lock(self.ai.llm_cache().read());
            if let Some(service) = cache.as_ref() {
                return Ok(Arc::clone(service));
            }
        }

        // SLOW PATH: one caller loads while peers await the per-role lock.
        let llm_service = self.load_llm_with_fallback().await?;

        // CACHE UPDATE: Store result with write lock (minimal critical section, poison-recovered)
        {
            let mut cache = Self::recover_write_lock(self.ai.llm_cache().write());

            // Double-check: Another thread may have loaded while we were loading
            if let Some(cached_service) = cache.as_ref() {
                // Race condition: Another thread won. Use their result instead.
                // Our loaded service will be dropped (wasteful but safe).
                return Ok(Arc::clone(cached_service));
            }
            *cache = Some(Arc::clone(&llm_service));
        } // ← Write lock released

        Ok(llm_service)
    }

    /// Gets the router LLM service, loading it lazily if needed.
    ///
    /// Uses a separate cache so the router can use a smaller model.
    pub async fn get_or_load_router_llm(&self) -> Result<Arc<dyn LLMPort>> {
        let settings = self
            .system
            .get_settings_use_case()
            .execute()
            .await
            .map_err(|e| AppError::InvalidConfig(format!("Failed to load settings: {}", e)))?;

        let router_settings = settings.llm.router.clone();

        if !router_settings.enabled {
            return Err(AppError::InvalidConfig(
                "Router is required for follow-up handling and cannot be disabled.".to_string(),
            ));
        }

        if router_settings.model.trim().is_empty() {
            return Err(AppError::InvalidConfig(
                "Router model is required. Configure llm.router.model in Settings → Model Catalog."
                    .to_string(),
            ));
        }

        let model_name = router_settings.model.trim().to_string();

        {
            let cache = Self::recover_read_lock(self.router_llm_cache.read());
            if let Some((cached_model, cached_llm)) = cache.as_ref() {
                if cached_model == &model_name {
                    return Ok(Arc::clone(cached_llm));
                }
            }
        }

        let _load_guard = self.router_llm_load_lock.lock().await;
        {
            let cache = Self::recover_read_lock(self.router_llm_cache.read());
            if let Some((cached_model, cached_llm)) = cache.as_ref() {
                if cached_model == &model_name {
                    return Ok(Arc::clone(cached_llm));
                }
            }
        }

        let generation_config = crate::llm::GenerationConfig {
            temperature: router_settings.temperature,
            top_p: settings.llm.top_p,
            top_k: settings.llm.top_k,
            max_tokens: router_settings.max_tokens as usize,
            repeat_penalty: settings.llm.repeat_penalty,
        };

        // 1) Try local downloaded model by ID (if present).
        if let Some(local_llm) = self
            .try_load_router_local_model(&model_name, generation_config.clone())
            .await?
        {
            {
                let mut cache = Self::recover_write_lock(self.router_llm_cache.write());
                *cache = Some((model_name.clone(), Arc::clone(&local_llm)));
            }
            return Ok(local_llm);
        }

        // 2) If local model not found, try Ollama if configured.
        let loaded = match settings.llm.provider {
            crate::features::settings::dto::LLMProvider::Local => {
                return Err(AppError::AiModelsNotInstalled(format!(
                    "Router model '{}' is not downloaded. Download it in Settings → Model Catalog.",
                    model_name
                )));
            }
            crate::features::settings::dto::LLMProvider::Ollama
            | crate::features::settings::dto::LLMProvider::Auto => {
                self.try_load_ollama_with_model(&settings.llm, &model_name, generation_config)
                    .await
                    .map_err(|e| {
                        AppError::AiModelsNotInstalled(format!(
                            "Router model '{}' is not available locally and Ollama failed: {}. \
                             Download the router model in Settings → Model Catalog or configure Ollama.",
                            model_name, e
                        ))
                    })?
            }
        };

        {
            let mut cache = Self::recover_write_lock(self.router_llm_cache.write());
            *cache = Some((model_name, Arc::clone(&loaded)));
        }

        Ok(loaded)
    }

    async fn try_load_router_local_model(
        &self,
        model_id: &str,
        generation_config: crate::llm::GenerationConfig,
    ) -> Result<Option<Arc<dyn LLMPort>>> {
        use crate::infrastructure::llm::factory::{create_llm, LLMConfig};

        let maybe_model = self
            .ai
            .downloaded_model_repo()
            .find_by_model_id(model_id)
            .await?;

        let model = match maybe_model {
            Some(model) => model,
            None => return Ok(None),
        };

        if let Err(reason) = model.validate_for_operation(true) {
            return Err(AppError::InvalidConfig(reason));
        }

        let model_path = match model.loadable_path() {
            Some(p) => p.to_path_buf(),
            None => return Ok(None),
        };
        if !model_path.exists() {
            return Ok(None);
        }

        tracing::info!(
            router_model = %model.model_name(),
            path = %model_path.display(),
            "Loading router model from local GGUF"
        );

        let llm_config = LLMConfig::Local {
            model_path,
            n_gpu_layers: -1,
            generation_config,
            // Populated when the app boots via Container::with_app_handle.
            // None in tests / sidecar-feature-off builds (where it's unused).
            app_handle: self.app_handle(),
        };

        match create_llm(llm_config).await {
            Ok(llm) => Ok(Some(llm)),
            Err(e) => Err(AppError::ModelLoadFailed(format!(
                "Failed to load router model '{}': {}",
                model.model_name(),
                e
            ))),
        }
    }

    /// Get the utility LLM if one is configured.
    pub async fn get_or_load_utility_llm(&self) -> Result<Option<Arc<dyn LLMPort>>> {
        let active = match self
            .ai
            .downloaded_model_repo()
            .get_active_utility_model()
            .await
        {
            Ok(Some(m)) => m,
            Ok(None) => return Ok(None),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Failed to query active utility model — falling back to chat LLM"
                );
                return Ok(None);
            }
        };

        let settings = self
            .system
            .get_settings_use_case()
            .execute()
            .await
            .map_err(|e| AppError::InvalidConfig(format!("Failed to load settings: {}", e)))?;

        let generation_config = crate::llm::GenerationConfig {
            temperature: settings.llm.temperature,
            top_p: settings.llm.top_p,
            top_k: settings.llm.top_k,
            max_tokens: 512,
            repeat_penalty: settings.llm.repeat_penalty,
        };

        let ollama_utility_tag = {
            let utility = settings.llm.ollama_utility_model.trim();
            if utility.is_empty() {
                settings.llm.model.clone()
            } else {
                utility.to_string()
            }
        };

        let cache_key = if active.location().is_local() {
            active.model_id().to_string()
        } else {
            format!("{}::{}", active.model_id(), ollama_utility_tag)
        };

        // Fast path: return cached LLM if cache key matches.
        {
            let cache = Self::recover_read_lock(self.utility_llm_cache.read());
            if let Some((cached_model, cached_llm)) = cache.as_ref() {
                if cached_model == &cache_key {
                    return Ok(Some(Arc::clone(cached_llm)));
                }
            }
        }

        let _load_guard = self.utility_llm_load_lock.lock().await;
        {
            let cache = Self::recover_read_lock(self.utility_llm_cache.read());
            if let Some((cached_model, cached_llm)) = cache.as_ref() {
                if cached_model == &cache_key {
                    return Ok(Some(Arc::clone(cached_llm)));
                }
            }
        }

        let llm_result = if active.location().is_local() {
            self.load_utility_local(&active, generation_config).await
        } else {
            self.try_load_ollama_with_model(&settings.llm, &ollama_utility_tag, generation_config)
                .await
                .map(Some)
                .or_else(|e| {
                    tracing::warn!(
                        model_id = %active.model_id(),
                        ollama_tag = %ollama_utility_tag,
                        error = %e,
                        "Failed to reach Ollama for utility role — falling back to chat LLM"
                    );
                    Ok(None)
                })
        };

        let llm = match llm_result? {
            Some(llm) => llm,
            None => return Ok(None),
        };

        {
            let mut cache = Self::recover_write_lock(self.utility_llm_cache.write());
            *cache = Some((cache_key, Arc::clone(&llm)));
        }

        Ok(Some(llm))
    }

    async fn load_utility_local(
        &self,
        active: &crate::domain::DownloadedModel,
        generation_config: crate::llm::GenerationConfig,
    ) -> Result<Option<Arc<dyn LLMPort>>> {
        let model_id = active.model_id();
        let model_path = match active.loadable_path() {
            Some(p) => p.to_path_buf(),
            None => {
                tracing::warn!(
                    model_id = %model_id,
                    "Active utility model has no loadable path — skipping"
                );
                return Ok(None);
            }
        };
        if !model_path.exists() {
            tracing::warn!(
                model_id = %model_id,
                path = %model_path.display(),
                "Active utility model artifact missing on disk — falling back to chat LLM"
            );
            return Ok(None);
        }

        let llm_config = crate::infrastructure::llm::factory::LLMConfig::Local {
            model_path: model_path.clone(),
            n_gpu_layers: -1,
            generation_config,
            app_handle: self.app_handle(),
        };

        tracing::info!(
            model_id = %model_id,
            path = %model_path.display(),
            "Loading utility LLM"
        );

        match crate::infrastructure::llm::factory::create_llm(llm_config).await {
            Ok(llm) => Ok(Some(llm)),
            Err(e) => {
                tracing::warn!(
                    model_id = %model_id,
                    error = %e,
                    "Failed to load utility LLM — falling back to chat LLM"
                );
                Ok(None)
            }
        }
    }

    pub fn invalidate_utility_llm_cache(&self) {
        let mut cache = Self::recover_write_lock(self.utility_llm_cache.write());
        if cache.is_some() {
            *cache = None;
            tracing::debug!("Utility LLM cache invalidated");
        }
    }

    /// Fire-and-forget per-role warmup at boot. Emits
    /// `model:warmup-status { role, phase, error? }` events with phases
    /// `started → ready | skipped | failed`. Errors never propagate.
    pub fn prewarm_active_models(&self, app_handle: tauri::AppHandle) {
        use tauri::Manager;
        for role in ["chat", "utility", "embedding"] {
            let handle = app_handle.clone();
            tokio::spawn(async move {
                let container = match handle.try_state::<Container>() {
                    Some(c) => c,
                    None => {
                        tracing::warn!(role, "prewarm: Container not in Tauri state yet");
                        return;
                    }
                };
                emit_warmup(&handle, role, "started", None);
                let started = std::time::Instant::now();
                let outcome = match role {
                    "chat" => match container.get_or_load_llm().await {
                        Ok(_) => Outcome::Ready,
                        Err(e) => classify_load_error(e),
                    },
                    "utility" => match container.get_or_load_utility_llm().await {
                        Ok(Some(_)) => Outcome::Ready,
                        Ok(None) => Outcome::Skipped,
                        Err(e) => classify_load_error(e),
                    },
                    "embedding" => match container.get_or_load_embedding().await {
                        Ok(_) => Outcome::Ready,
                        Err(e) => classify_load_error(e),
                    },
                    _ => unreachable!(),
                };
                let elapsed_ms = started.elapsed().as_millis() as u64;
                match &outcome {
                    Outcome::Ready => tracing::info!(role, elapsed_ms, "prewarm: ready"),
                    Outcome::Skipped => tracing::info!(role, "prewarm: no active model — skipped"),
                    Outcome::Failed(msg) => tracing::warn!(role, error = %msg, "prewarm: failed"),
                }
                let (phase, error) = match outcome {
                    Outcome::Ready => ("ready", None),
                    Outcome::Skipped => ("skipped", None),
                    Outcome::Failed(msg) => ("failed", Some(msg)),
                };
                emit_warmup(&handle, role, phase, error);
            });
        }
    }

    /// Invalidate the LLM cache
    ///
    /// Call this when the active model changes to force reload on next access
    pub fn invalidate_llm_cache(&self) {
        let mut cache = Self::recover_write_lock(self.ai.llm_cache().write());
        if cache.is_some() {
            tracing::info!("Invalidating LLM cache - next access will reload");
            *cache = None;
        }
    }

    /// Invalidate the router LLM cache.
    pub fn invalidate_router_llm_cache(&self) {
        let mut cache = Self::recover_write_lock(self.router_llm_cache.write());
        if cache.is_some() {
            tracing::info!("Invalidating router LLM cache - next access will reload");
            *cache = None;
        }
    }

    /// Refresh runtime custom tool handlers from current settings.
    ///
    /// This updates the function executor immediately so conversation tool calls
    /// reflect settings changes without requiring an app restart.
    pub async fn refresh_custom_tools_from_settings(&self) {
        match self.system.get_settings_use_case().execute().await {
            Ok(settings) => {
                let custom_tool_map: HashMap<
                    String,
                    crate::features::settings::dto::CustomToolSettingsDto,
                > = settings
                    .llm
                    .custom_tools
                    .into_iter()
                    .filter(|tool| tool.enabled)
                    .map(|tool| (tool.name.clone(), tool))
                    .collect();

                self.function_executor.set_custom_tools(custom_tool_map);
            }
            Err(error) => {
                tracing::warn!(
                    "Failed to refresh custom tools from settings; keeping previous configuration: {}",
                    error
                );
            }
        }
    }

    /// Try to load the active downloaded model
    ///
    /// Returns Ok(Some(llm)) if active model loaded successfully
    /// Returns Ok(None) if no active model or file doesn't exist
    /// Returns Err only on unrecoverable errors
    fn generation_config_from_settings(
        settings: &crate::features::settings::dto::LLMSettingsDto,
    ) -> crate::llm::GenerationConfig {
        crate::llm::GenerationConfig {
            temperature: settings.temperature,
            top_p: settings.top_p,
            top_k: settings.top_k,
            max_tokens: settings.max_tokens as usize,
            repeat_penalty: settings.repeat_penalty,
        }
    }

    async fn try_load_active_model(
        &self,
        generation_config: crate::llm::GenerationConfig,
    ) -> Result<Option<Arc<dyn LLMPort>>> {
        use crate::infrastructure::llm::factory::{create_llm, LLMConfig};

        // Check if there's an active model in the database (from AIModule)
        let active_model = match self
            .ai
            .downloaded_model_repo()
            .get_active_chat_model()
            .await
        {
            Ok(Some(model)) => model,
            Ok(None) => {
                tracing::debug!("No active model configured in database");
                return Ok(None);
            }
            Err(e) => {
                tracing::warn!("Failed to query active model from database: {}", e);
                return Ok(None);
            }
        };

        tracing::info!(
            "Found active model in database: {} ({})",
            active_model.model_name(),
            active_model.model_id()
        );

        let model_path = match active_model.loadable_path() {
            Some(p) => p.to_path_buf(),
            None => return Ok(None),
        };

        if !model_path.exists() {
            tracing::warn!(
                "Active model artifact not found: {} - user may have deleted it manually",
                model_path.display()
            );
            return Ok(None);
        }

        tracing::info!("Loading GGUF model from: {}", model_path.display());

        // Attempt to load the GGUF model
        let llm_config = LLMConfig::Local {
            model_path: model_path.clone(),
            n_gpu_layers: -1, // Use Metal GPU on M-series Macs (all layers on GPU)
            generation_config,
            // Populated when the app boots via Container::with_app_handle.
            // None in tests / sidecar-feature-off builds (where it's unused).
            app_handle: self.app_handle(),
        };

        match create_llm(llm_config).await {
            Ok(llm) => {
                tracing::info!(
                    "✅ Successfully loaded active model: {}",
                    active_model.model_name()
                );
                Ok(Some(llm))
            }
            Err(e) => {
                tracing::error!(
                    "Failed to load active model {}: {}",
                    active_model.model_name(),
                    e
                );

                // Return actual error instead of swallowing
                Err(AppError::ModelLoadFailed(format!(
                    "Failed to load model '{}': {}. \
                     The model file may be corrupted or incompatible. \
                     Try downloading a different model from Settings → Model Catalog.",
                    active_model.model_name(),
                    e
                )))
            }
        }
    }

    /// Load LLM with fallback chain
    ///
    /// Strategy:
    /// 1. Try active model from models DB table (GGUF)
    /// 2. Try Ollama from config (if configured)
    /// 3. Return clear error if no model exists
    ///
    /// This allows using either local downloaded models OR Ollama server.
    async fn load_llm_with_fallback(&self) -> Result<Arc<dyn LLMPort>> {
        let settings = self
            .system
            .get_settings_use_case()
            .execute()
            .await
            .map_err(|e| AppError::InvalidConfig(format!("Failed to load settings: {}", e)))?;

        let generation_config = Self::generation_config_from_settings(&settings.llm);

        match settings.llm.provider {
            LLMProvider::Local => {
                tracing::debug!("LLM provider set to local; attempting to load active model...");
                match self.try_load_active_model(generation_config.clone()).await {
                    Ok(Some(llm)) => Ok(llm),
                    Ok(None) => Err(AppError::AiModelsNotInstalled(
                        "No local model available. Download and activate a model in Settings → Model Catalog."
                            .to_string(),
                    )),
                    Err(e) => Err(e),
                }
            }
            LLMProvider::Ollama => {
                tracing::debug!("LLM provider set to Ollama; attempting to load Ollama client...");
                self.try_load_ollama(&settings.llm, generation_config).await
            }
            LLMProvider::Auto => {
                // STRATEGY 1: Try active downloaded model (local GGUF)
                tracing::debug!("Attempting to load active downloaded model...");

                match self.try_load_active_model(generation_config.clone()).await {
                    Ok(Some(llm)) => {
                        tracing::info!("✅ Using active downloaded model");
                        return Ok(llm);
                    }
                    Ok(None) => {
                        tracing::debug!(
                            "No active downloaded model configured, trying Ollama fallback..."
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load active model: {}, trying Ollama fallback...",
                            e
                        );
                    }
                }

                // STRATEGY 2: Try Ollama from config
                tracing::debug!("Attempting to load Ollama client from config...");

                match self
                    .try_load_ollama(&settings.llm, generation_config.clone())
                    .await
                {
                    Ok(llm) => {
                        tracing::info!("✅ Using Ollama server");
                        return Ok(llm);
                    }
                    Err(e) => {
                        tracing::debug!("Ollama not available: {}", e);
                    }
                }

                // STRATEGY 3: No options available - return error
                let error_msg = "No LLM available. Please either:\n\
                                 1. Download and activate a model in Settings → Model Catalog, OR\n\
                                 2. Configure Ollama server in Settings → LLM Configuration";
                tracing::error!("{}", error_msg);
                Err(AppError::AiModelsNotInstalled(error_msg.to_string()))
            }
        }
    }

    /// Try to load Ollama client from config
    ///
    /// Returns Ok(llm) if Ollama is configured and available
    /// Returns Err if config missing, invalid, or Ollama not reachable
    async fn try_load_ollama(
        &self,
        llm_settings: &crate::features::settings::dto::LLMSettingsDto,
        generation_config: crate::llm::GenerationConfig,
    ) -> Result<Arc<dyn LLMPort>> {
        use crate::infrastructure::llm::factory::create_ollama_llm;

        // Check if Ollama endpoint is configured
        let endpoint = llm_settings.ollama_url.clone();
        let model = llm_settings.model.clone();
        let auth_header_name = llm_settings.ollama_auth_header_name.trim();
        let auth_header_value = llm_settings.ollama_auth_header_value.trim();
        let auth_header = if !auth_header_name.is_empty() && !auth_header_value.is_empty() {
            Some((auth_header_name.to_string(), auth_header_value.to_string()))
        } else {
            None
        };

        if endpoint.is_empty() || model.is_empty() {
            return Err(AppError::InvalidConfig(
                "Ollama endpoint or model not configured".to_string(),
            ));
        }

        tracing::info!(
            "Attempting to connect to Ollama at {} with model {}",
            endpoint,
            model
        );

        // Try to create Ollama client (includes health check)
        match create_ollama_llm(&endpoint, &model, auth_header, generation_config).await {
            Ok(llm) => {
                tracing::info!("✅ Successfully connected to Ollama server");
                Ok(llm)
            }
            Err(e) => {
                tracing::warn!("Failed to connect to Ollama: {}", e);
                Err(AppError::ServiceNotAvailable(format!(
                    "Ollama server not available at {}: {}. \
                     Make sure Ollama is running (ollama serve) and the model '{}' is pulled (ollama pull {}).",
                    endpoint, e, model, model
                )))
            }
        }
    }

    /// Try to load Ollama client with an explicit model override.
    async fn try_load_ollama_with_model(
        &self,
        llm_settings: &crate::features::settings::dto::LLMSettingsDto,
        model: &str,
        generation_config: crate::llm::GenerationConfig,
    ) -> Result<Arc<dyn LLMPort>> {
        use crate::infrastructure::llm::factory::create_ollama_llm;

        let endpoint = llm_settings.ollama_url.clone();
        let auth_header_name = llm_settings.ollama_auth_header_name.trim();
        let auth_header_value = llm_settings.ollama_auth_header_value.trim();
        let auth_header = if !auth_header_name.is_empty() && !auth_header_value.is_empty() {
            Some((auth_header_name.to_string(), auth_header_value.to_string()))
        } else {
            None
        };

        if endpoint.is_empty() || model.trim().is_empty() {
            return Err(AppError::InvalidConfig(
                "Ollama endpoint or router model not configured".to_string(),
            ));
        }

        tracing::info!(
            "Attempting to connect to Ollama at {} with router model {}",
            endpoint,
            model
        );

        match create_ollama_llm(&endpoint, model, auth_header, generation_config).await {
            Ok(llm) => {
                tracing::info!("✅ Successfully connected to Ollama router model");
                Ok(llm)
            }
            Err(e) => {
                tracing::warn!("Failed to connect to Ollama router model: {}", e);
                Err(AppError::ServiceNotAvailable(format!(
                    "Ollama router model '{}' not available at {}: {}",
                    model, endpoint, e
                )))
            }
        }
    }

    // === Infrastructure Getters (delegate to CoreModule) ===

    pub fn db_pool(&self) -> &SqlitePool {
        self.core.db_pool()
    }

    pub fn db_conn(
        &self,
    ) -> &Arc<crate::infrastructure::persistence::database::DatabaseConnection> {
        self.core.db_conn()
    }

    pub fn security_context(&self) -> &Arc<SecurityContext> {
        self.core.security_context()
    }

    pub fn function_registry(&self) -> &Arc<dyn FunctionRegistryTrait> {
        &self.function_registry
    }

    pub fn function_executor(&self) -> &Arc<dyn FunctionExecutorTrait> {
        &self.function_executor
    }

    /// Get file access configuration (for secure path validation)
    ///
    /// Returns Arc-wrapped FileAccessConfig for use by file operation use cases.
    /// This enforces path validation against allowed_roots (lattice + indexed directories).
    pub fn file_access_config(&self) -> Arc<FileAccessConfig> {
        Arc::clone(self.core.file_access_config())
    }

    /// Gets the embedding service, loading it lazily if needed.
    ///
    /// Uses double-check locking pattern to minimize lock contention:
    /// 1. Fast path: Check cache with read lock (no loading)
    /// 2. Slow path: Load outside any lock, then store with write lock
    ///
    /// Thread-safe: Multiple threads may load simultaneously, but only one
    /// result is cached (last writer wins, which is safe since all loads
    /// produce equivalent services).
    pub async fn get_or_load_embedding(&self) -> Result<Arc<dyn EmbeddingPort>> {
        // Delegate to SearchModule's embedding_cache (also shared with IndexingModule, via getter)
        // FAST PATH: Check if already cached (read lock with poison recovery)
        let cached_service = {
            let cache = Self::recover_read_lock(self.search.embedding_cache().read());
            cache.as_ref().cloned()
        }; // ← Read lock released before I/O

        if let Some(service) = cached_service {
            match service.is_ready().await {
                Ok(true) => return Ok(service),
                Ok(false) => {
                    tracing::info!("Cached embedding service is not ready; invalidating cache");
                    self.invalidate_embedding_cache();
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to check cached embedding readiness; invalidating cache: {}",
                        e
                    );
                    self.invalidate_embedding_cache();
                }
            }
        }

        let _load_guard = self.embedding_load_lock.lock().await;
        let cached_service = {
            let cache = Self::recover_read_lock(self.search.embedding_cache().read());
            cache.as_ref().cloned()
        };
        if let Some(service) = cached_service {
            match service.is_ready().await {
                Ok(true) => return Ok(service),
                Ok(false) => self.invalidate_embedding_cache(),
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        "Failed to check cached embedding readiness after awaiting load lock"
                    );
                    self.invalidate_embedding_cache();
                }
            }
        }

        // Check error cooldown: if we recently failed to load, return the cached
        // error immediately instead of retrying (prevents log spam).
        const EMBEDDING_ERROR_COOLDOWN_SECS: u64 = 60;
        {
            let cooldown = self.embedding_error_cooldown.read();
            if let Some((failed_at, ref cached_err)) = *cooldown {
                if failed_at.elapsed().as_secs() < EMBEDDING_ERROR_COOLDOWN_SECS {
                    return Err(cached_err.clone());
                }
            }
        }

        // SLOW PATH: one caller loads while peers await the per-role lock.
        let embedding_service = match self.load_embedding_with_fallback().await {
            Ok(service) => {
                // Clear any cached error on success
                let mut cooldown = self.embedding_error_cooldown.write();
                *cooldown = None;
                service
            }
            Err(e) => {
                // Cache the error with a timestamp to prevent retry spam
                let mut cooldown = self.embedding_error_cooldown.write();
                *cooldown = Some((Instant::now(), e.clone()));
                return Err(e);
            }
        };

        // Cache only real/ready embedding services.
        // Degraded mock services are intentionally NOT cached to avoid stale mock lock-in.
        let should_cache = match embedding_service.is_ready().await {
            Ok(true) => true,
            Ok(false) => false,
            Err(e) => {
                tracing::warn!(
                    "Loaded embedding service readiness check failed; not caching: {}",
                    e
                );
                false
            }
        };

        if should_cache {
            let mut cache = Self::recover_write_lock(self.search.embedding_cache().write());

            // Double-check: another thread may have loaded while we were loading.
            if let Some(cached_service) = cache.as_ref() {
                return Ok(Arc::clone(cached_service));
            }

            *cache = Some(Arc::clone(&embedding_service));
        }

        Ok(embedding_service)
    }

    /// Invalidate the embedding service cache
    ///
    /// Call this when the active embedding model changes to force reload on next access
    pub fn invalidate_embedding_cache(&self) {
        let mut cache = Self::recover_write_lock(self.search.embedding_cache().write());
        if cache.is_some() {
            tracing::info!("Invalidating embedding cache - next access will reload");
            *cache = None;
        }
        // Also clear error cooldown so the next attempt retries immediately
        let mut cooldown = self.embedding_error_cooldown.write();
        *cooldown = None;
    }

    /// Try to load the active embedding model
    ///
    /// Returns Ok(Some(embedding)) if active model loaded successfully
    /// Returns Ok(None) if no active model or file doesn't exist
    /// Returns Err only on unrecoverable errors
    async fn try_load_active_embedding_model(&self) -> Result<Option<Arc<dyn EmbeddingPort>>> {
        // Check if there's an active embedding model in the database (from AIModule)
        let active_model = match self
            .ai
            .downloaded_model_repo()
            .get_active_embedding_model()
            .await
        {
            Ok(Some(model)) => model,
            Ok(None) => {
                tracing::debug!("No active embedding model configured in database");
                return Ok(None);
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to query active embedding model from database: {}",
                    e
                );
                return Ok(None);
            }
        };

        tracing::info!(
            "Found active embedding model in database: {} ({})",
            active_model.model_name(),
            active_model.model_id()
        );

        let model_path = match active_model.loadable_path() {
            Some(p) => p.to_path_buf(),
            None => return Ok(None),
        };

        if !model_path.exists() {
            tracing::warn!(
                "Active embedding model artifact not found: {} - user may have deleted it manually",
                model_path.display()
            );
            return Ok(None);
        }

        let models_dir = model_path
            .parent()
            .ok_or_else(|| AppError::Security("Model path has no parent directory".into()))?
            .to_path_buf();

        let validated_path = match self
            .core
            .security_context()
            .input_validator()
            .validate_model_path(&model_path, &models_dir)
        {
            Ok(path) => path,
            Err(e) => {
                tracing::error!(
                    "Path validation failed for model {}: {}",
                    active_model.model_name(),
                    e
                );
                tracing::error!(
                    "Rejecting model load due to security violation (CWE-22: Path Traversal)"
                );
                return Ok(None);
            }
        };

        let model_dir = active_model.location().enclosing_dir().ok_or_else(|| {
            AppError::ModelLoadFailed(format!(
                "Active embedding model {} has no enclosing directory (remote-hosted?)",
                active_model.model_name()
            ))
        })?;

        tracing::info!(
            "Loading Candle embedding model from validated dir: {}",
            model_dir.display()
        );

        match CandleEmbeddingService::new(&model_dir) {
            Ok(service) => {
                let expected_dim = self.search.vector_search().dimension();
                let actual_dim = service.dimension();
                if actual_dim != expected_dim {
                    tracing::error!(
                        "Embedding dimension mismatch: model '{}' produces {}-dim vectors \
                         but the vector index expects {}-dim. Restart the app to rebuild \
                         the index at the new dimension.",
                        active_model.model_name(),
                        actual_dim,
                        expected_dim
                    );
                    return Err(AppError::ModelLoadFailed(format!(
                        "Model '{}' produces {}-dimensional embeddings but the search index \
                         requires {}. Restart the app to migrate the index.",
                        active_model.model_name(),
                        actual_dim,
                        expected_dim
                    )));
                }

                tracing::info!(
                    "Successfully loaded active embedding model: {} ({}-dim, {:?})",
                    active_model.model_name(),
                    actual_dim,
                    service.architecture()
                );
                Ok(Some(Arc::new(service) as Arc<dyn EmbeddingPort>))
            }
            Err(e) => {
                tracing::error!(
                    "Failed to load active embedding model {}: {}",
                    active_model.model_name(),
                    e
                );
                Err(AppError::ModelLoadFailed(format!(
                    "Failed to load active embedding model '{}' from '{}': {}",
                    active_model.model_name(),
                    model_dir.display(),
                    e
                )))
            }
        }
    }

    async fn load_embedding_with_fallback(&self) -> Result<Arc<dyn EmbeddingPort>> {
        use crate::application::ports::MockEmbeddingPort;

        // Step 1: Try active downloaded embedding model
        tracing::debug!("Step 1: Checking for active downloaded embedding model...");
        match self.try_load_active_embedding_model().await {
            Ok(Some(embedding)) => {
                tracing::info!("Using active downloaded embedding model");
                return Ok(embedding);
            }
            Ok(None) => {
                tracing::debug!("No active embedding model available, using mock...");
            }
            Err(e @ AppError::ModelLoadFailed(_)) => {
                return Err(e);
            }
            Err(e) => {
                tracing::warn!("Error loading active embedding model: {}, using mock...", e);
            }
        }

        // Step 2: Fallback to Mock (ALWAYS succeeds)
        tracing::info!("Using Mock embedding implementation");
        tracing::info!("💡 Download an embedding model to enable semantic search features");
        Ok(Arc::new(MockEmbeddingPort::new_degraded()) as Arc<dyn EmbeddingPort>)
    }

    /// Get embedding service (deprecated - use get_or_load_embedding().await)
    ///
    /// This is kept for backward compatibility but should be replaced with get_or_load_embedding()
    #[deprecated(note = "Use get_or_load_embedding().await instead")]
    pub fn embedding_service(&self) -> Option<Arc<dyn EmbeddingPort>> {
        // Return None to force callers to migrate to async pattern
        None
    }

    /// Get LLM service with lazy loading
    #[deprecated(note = "Use get_or_load_llm().await instead")]
    pub async fn llm_service(&self) -> Result<Arc<dyn LLMPort>> {
        self.get_or_load_llm().await
    }

    // === Application Use Case Getters (delegate to modules) ===

    // Health (from SystemModule)
    pub fn health_check_use_case(&self) -> Arc<HealthCheckUseCase> {
        Arc::clone(self.system.health_check_use_case())
    }

    pub fn initialize_database_use_case(&self) -> Arc<InitializeDatabaseUseCase> {
        Arc::clone(self.system.initialize_database_use_case())
    }

    pub fn initialize_models_use_case(&self) -> Arc<InitializeModelsUseCase> {
        Arc::clone(self.system.initialize_models_use_case())
    }

    // Search (from SearchModule)
    pub fn semantic_search_use_case(&self) -> Arc<SemanticSearchUseCase> {
        Arc::clone(self.search.semantic_search_use_case())
    }

    pub fn hybrid_search_use_case(&self) -> Arc<HybridSearchUseCase> {
        Arc::clone(self.search.hybrid_search_use_case())
    }

    // Indexing (from IndexingModule)
    pub fn index_file_use_case(&self) -> Arc<IndexFileUseCase> {
        Arc::clone(self.indexing.index_file_use_case())
    }

    pub fn index_directory_use_case(&self) -> Arc<IndexDirectoryUseCase> {
        Arc::clone(self.indexing.index_directory_use_case())
    }

    pub fn reindex_document_use_case(&self) -> Arc<ReindexDocumentUseCase> {
        Arc::clone(self.indexing.reindex_document_use_case())
    }

    pub fn delete_document_use_case(&self) -> Arc<DeleteDocumentUseCase> {
        Arc::clone(self.indexing.delete_document_use_case())
    }

    pub fn rename_document_use_case(&self) -> Arc<RenameDocumentUseCase> {
        Arc::clone(self.indexing.rename_document_use_case())
    }

    // Q&A (uses vector_search and document_repo from SearchModule)
    pub async fn ask_question_use_case(&self) -> Result<Arc<AskQuestionUseCase>> {
        let llm = self.get_or_load_llm().await?;

        let hyde_service = Arc::new(crate::infrastructure::services::hyde::HyDEService::new(
            Arc::clone(&llm),
        ));

        let qa_embedding = self.get_or_load_embedding().await.unwrap_or_else(|_| {
            use crate::application::ports::MockEmbeddingPort;
            Arc::new(MockEmbeddingPort::new_degraded()) as Arc<dyn EmbeddingPort>
        });

        Ok(Arc::new(AskQuestionUseCase::new(
            hyde_service,
            qa_embedding,
            Arc::clone(self.search.vector_search()),
            llm,
            Arc::clone(self.search.document_repo())
                as Arc<dyn crate::application::ports::DocumentRepositoryPort>,
            Arc::clone(self.indexing.chunk_repository()),
        )))
    }

    // Tags (from LibraryModule, except AI-powered tags from AIModule)
    pub fn create_tag_use_case(&self) -> Arc<CreateTagUseCase> {
        Arc::clone(self.library.create_tag_use_case())
    }

    pub fn update_tag_use_case(&self) -> Arc<UpdateTagUseCase> {
        Arc::clone(self.library.update_tag_use_case())
    }

    pub fn delete_tag_use_case(&self) -> Arc<DeleteTagUseCase> {
        Arc::clone(self.library.delete_tag_use_case())
    }

    pub fn remove_tag_from_document_use_case(&self) -> Arc<RemoveTagFromDocumentUseCase> {
        Arc::clone(self.library.remove_tag_from_document_use_case())
    }

    pub fn get_tags_use_case(&self) -> Arc<GetTagsUseCase> {
        Arc::clone(self.library.get_tags_use_case())
    }

    pub fn apply_tags_use_case(&self) -> Arc<ApplyTagsUseCase> {
        Arc::clone(self.library.apply_tags_use_case())
    }

    pub fn search_by_tag_use_case(&self) -> Arc<SearchByTagUseCase> {
        Arc::clone(self.library.search_by_tag_use_case())
    }

    // AI-powered tags (from AIModule)
    pub fn generate_tags_use_case(&self) -> Arc<GenerateTagsUseCase> {
        Arc::clone(self.ai.generate_tags_use_case())
    }

    pub fn auto_tag_all_documents_use_case(&self) -> Arc<AutoTagAllDocumentsUseCase> {
        Arc::clone(self.ai.auto_tag_all_documents_use_case())
    }

    // Stats (from SystemModule)
    pub fn get_system_stats_use_case(&self) -> Arc<GetSystemStatsUseCase> {
        Arc::clone(self.system.get_system_stats_use_case())
    }

    pub fn get_corpus_shape_use_case(&self) -> Arc<GetCorpusShapeUseCase> {
        Arc::clone(self.system.get_corpus_shape_use_case())
    }

    // Conversations (from AIModule)
    pub fn create_conversation_use_case(&self) -> Arc<CreateConversationUseCase> {
        Arc::clone(self.ai.create_conversation_use_case())
    }

    // Settings (from SystemModule)
    pub fn get_settings_use_case(&self) -> Arc<GetSettingsUseCase> {
        Arc::clone(self.system.get_settings_use_case())
    }

    /// Settings update use case, wired with the side-effects port so
    /// LLM cache invalidation fires after a successful write.
    ///
    /// Constructs a fresh use case on each call rather than returning
    /// the pre-built `Arc` from `SystemModule`. SystemModule's stored
    /// instance is built before the side-effects port exists
    /// (bootstrap order: System → AI → side-effects), so it has only
    /// noop side effects. Production code paths should always go
    /// through Container, never directly through SystemModule.
    /// Audit P0-3 fix.
    pub fn update_settings_use_case(&self) -> Arc<UpdateSettingsUseCase> {
        Arc::new(UpdateSettingsUseCase::with_side_effects(
            self.system.settings_repo().clone(),
            self.settings_side_effects.clone(),
        ))
    }

    /// Settings reset use case, wired with the side-effects port. See
    /// `update_settings_use_case` for the bootstrap-order rationale.
    /// Audit P0-3 fix: reset previously did NOT invalidate any caches,
    /// so a "reset to defaults" left stale LLM caches until app
    /// restart.
    pub fn reset_settings_use_case(&self) -> Arc<ResetSettingsUseCase> {
        Arc::new(ResetSettingsUseCase::with_side_effects(
            self.system.settings_repo().clone(),
            self.settings_side_effects.clone(),
        ))
    }

    pub fn export_settings_use_case(&self) -> Arc<ExportSettingsUseCase> {
        Arc::clone(self.system.export_settings_use_case())
    }

    pub fn import_settings_use_case(&self) -> Arc<ImportSettingsUseCase> {
        Arc::clone(self.system.import_settings_use_case())
    }

    pub fn validate_settings_use_case(&self) -> Arc<ValidateSettingsUseCase> {
        Arc::clone(self.system.validate_settings_use_case())
    }

    // File Operations (from FileOpsModule)
    pub fn open_file_use_case(&self) -> Arc<OpenFileUseCase> {
        Arc::clone(self.file_ops.open_file_use_case())
    }

    pub fn open_file_by_id_use_case(&self) -> Arc<OpenFileByIdUseCase> {
        Arc::clone(self.file_ops.open_file_by_id_use_case())
    }

    pub fn get_file_path_by_id_use_case(&self) -> Arc<GetFilePathByIdUseCase> {
        Arc::clone(self.file_ops.get_file_path_by_id_use_case())
    }

    pub fn show_in_folder_use_case(&self) -> Arc<ShowInFolderUseCase> {
        Arc::clone(self.file_ops.show_in_folder_use_case())
    }

    pub fn get_file_metadata_use_case(&self) -> Arc<GetFileMetadataUseCase> {
        Arc::clone(self.file_ops.get_file_metadata_use_case())
    }

    pub fn read_file_content_use_case(&self) -> Arc<ReadFileContentUseCase> {
        Arc::clone(self.file_ops.read_file_content_use_case())
    }

    pub fn read_file_bytes_use_case(&self) -> Arc<ReadFileBytesUseCase> {
        Arc::clone(self.file_ops.read_file_bytes_use_case())
    }

    pub fn update_file_metadata_use_case(&self) -> Arc<UpdateFileMetadataUseCase> {
        Arc::clone(self.file_ops.update_file_metadata_use_case())
    }

    pub fn list_citing_conversations_use_case(&self) -> Arc<ListCitingConversationsUseCase> {
        Arc::clone(self.file_ops.list_citing_conversations_use_case())
    }

    // (Favorites + Recent: no use cases — Tauri commands route through
    // raw sqlx in their respective commands.rs files.)

    // Mentions (from LibraryModule)
    pub fn extract_mentions_use_case(&self) -> Arc<ExtractMentionsUseCase> {
        Arc::clone(self.library.extract_mentions_use_case())
    }

    pub fn search_mentions_use_case(&self) -> Arc<SearchMentionsUseCase> {
        Arc::clone(self.library.search_mentions_use_case())
    }

    pub fn get_backlinks_use_case(&self) -> Arc<GetBacklinksUseCase> {
        Arc::clone(self.library.get_backlinks_use_case())
    }

    pub fn get_mentions_by_type_use_case(&self) -> Arc<GetMentionsByTypeUseCase> {
        Arc::clone(self.library.get_mentions_by_type_use_case())
    }

    pub fn get_mentions_for_document_use_case(&self) -> Arc<GetMentionsForDocumentUseCase> {
        Arc::clone(self.library.get_mentions_for_document_use_case())
    }

    pub fn create_mention_use_case(&self) -> Arc<CreateMentionUseCase> {
        Arc::clone(self.library.create_mention_use_case())
    }

    pub fn delete_mention_use_case(&self) -> Arc<DeleteMentionUseCase> {
        Arc::clone(self.library.delete_mention_use_case())
    }

    // Extraction (from FileOpsModule)
    pub fn parse_wikilinks_use_case(&self) -> Arc<ParseWikilinksUseCase> {
        Arc::clone(self.file_ops.parse_wikilinks_use_case())
    }

    pub fn extract_document_title_use_case(&self) -> Arc<ExtractDocumentTitleUseCase> {
        Arc::clone(self.file_ops.extract_document_title_use_case())
    }

    pub fn resolve_wikilink_use_case(&self) -> Arc<ResolveWikilinkUseCase> {
        Arc::clone(self.file_ops.resolve_wikilink_use_case())
    }

    pub fn extract_and_resolve_links_use_case(&self) -> Arc<ExtractAndResolveLinksUseCase> {
        Arc::clone(self.file_ops.extract_and_resolve_links_use_case())
    }

    // Credentials (from CoreModule)
    pub fn set_api_key_use_case(&self) -> Arc<SetApiKeyUseCase> {
        Arc::clone(self.core.set_api_key_use_case())
    }

    pub fn get_api_key_use_case(&self) -> Arc<GetApiKeyUseCase> {
        Arc::clone(self.core.get_api_key_use_case())
    }

    pub fn delete_api_key_use_case(&self) -> Arc<DeleteApiKeyUseCase> {
        Arc::clone(self.core.delete_api_key_use_case())
    }

    pub fn set_custom_endpoint_use_case(&self) -> Arc<SetCustomEndpointUseCase> {
        Arc::clone(self.core.set_custom_endpoint_use_case())
    }

    // (Cache: no use cases — commands operate on the global QUERY_CACHE
    // singleton directly.)

    // Backup (from SystemModule)
    pub fn create_backup_use_case(&self) -> Arc<CreateBackupUseCase> {
        Arc::clone(self.system.create_backup_use_case())
    }

    pub fn restore_backup_use_case(&self) -> Arc<RestoreBackupUseCase> {
        Arc::clone(self.system.restore_backup_use_case())
    }

    /// The backup port. `plugin_list_backups` reads through this rather than
    /// the free `list_backups_impl`, which scans the wrong directory.
    pub fn backup_port(&self) -> Arc<dyn BackupPort> {
        Arc::clone(self.system.backup_port())
    }

    /// On-device speech-to-text. Shared with the content-extraction adapter so
    /// audio ingest and `transcribe_file` run through the same engine.
    pub fn transcription_port(&self) -> Arc<dyn crate::application::ports::TranscriptionPort> {
        Arc::clone(self.indexing.transcription_port())
    }

    pub fn start_auto_backup_use_case(&self) -> Arc<StartAutoBackupUseCase> {
        Arc::clone(self.system.start_auto_backup_use_case())
    }

    pub fn stop_auto_backup_use_case(&self) -> Arc<StopAutoBackupUseCase> {
        Arc::clone(self.system.stop_auto_backup_use_case())
    }

    pub fn startup_auto_backup_use_case(&self) -> Arc<StartupAutoBackupUseCase> {
        Arc::clone(self.system.startup_auto_backup_use_case())
    }

    // Updates (from SystemModule)
    pub fn check_for_updates_use_case(&self) -> Arc<CheckForUpdatesUseCase> {
        Arc::clone(self.system.check_for_updates_use_case())
    }

    pub fn get_current_version_use_case(&self) -> Arc<GetCurrentVersionUseCase> {
        Arc::clone(self.system.get_current_version_use_case())
    }

    // Metrics (from SystemModule)
    pub fn get_metrics_use_case(&self) -> Arc<GetMetricsUseCase> {
        Arc::clone(self.system.get_metrics_use_case())
    }

    // === OLD Infrastructure Service Getters (Backward Compatibility) ===
    // These are for legacy commands that haven't been migrated to DDD use cases yet

    /// Get vector search service (for legacy search commands, from SearchModule)
    pub fn search_service(&self) -> Arc<dyn SearchServiceTrait> {
        Arc::clone(self.search.search_service())
    }

    /// Get hybrid search service (for legacy search commands, from SearchModule)
    pub fn hybrid_search(&self) -> Arc<dyn HybridSearchTrait> {
        Arc::clone(self.search.hybrid_search_service())
    }

    /// Get search enrichment service (for legacy search commands, from SearchModule)
    pub fn search_enrichment_service(&self) -> Arc<dyn SearchEnrichmentServiceTrait> {
        Arc::clone(self.search.search_enrichment_service())
    }

    /// Get metrics service (for legacy commands - Note: Metrics is concrete type, not in modules)
    pub fn metrics(&self) -> Arc<Metrics> {
        Arc::clone(self.system.metrics_service())
    }

    /// Get conversation service (for conversation commands, from AIModule)
    pub fn conversation_service(&self) -> Arc<dyn ConversationServiceTrait> {
        Arc::clone(self.ai.conversation_service())
    }

    /// Get conversational Q&A service (for conversational Q&A commands, from AIModule)
    pub fn conversational_qa_service(&self) -> Arc<dyn ConversationalQAServiceTrait> {
        Arc::clone(self.ai.conversational_qa_service())
    }

    /// Get version info use case (alias for get_current_version_use_case)
    pub fn get_version_info_use_case(&self) -> Arc<GetCurrentVersionUseCase> {
        Arc::clone(self.system.get_current_version_use_case())
    }

    // Repository accessor for access tracking (from SearchModule)
    pub fn document_repository(&self) -> Arc<dyn DocumentRepository> {
        Arc::clone(self.search.document_repo())
    }

    /// Chunk repository accessor (from IndexingModule)
    pub fn chunk_repository(&self) -> Arc<dyn crate::application::ports::ChunkRepositoryPort> {
        Arc::clone(self.indexing.chunk_repository())
    }

    pub fn downloaded_model_repository(
        &self,
    ) -> Arc<crate::infrastructure::persistence::repositories::DownloadedModelRepository> {
        Arc::clone(self.ai.downloaded_model_repo())
    }

    pub fn mention_repository(&self) -> Arc<dyn MentionRepositoryPort> {
        Arc::clone(self.library.mention_repo())
    }

    /// Get web archive service (from IndexingModule)
    pub fn web_archive(&self) -> Arc<dyn WebArchiveServiceTrait> {
        Arc::clone(self.indexing.web_archive())
    }

    /// Get system info port (for model management - from SystemModule)
    pub fn system_info(&self) -> Arc<dyn SystemInfoPort> {
        Arc::clone(self.system.system_info())
    }

    /// Get model catalog service (Phase 2 - from AIModule)
    pub fn model_catalog(&self) -> Arc<dyn ModelCatalogPort> {
        Arc::clone(self.ai.model_catalog())
    }

    /// Get model catalog cache adapter (Hugging Face cache)
    pub fn model_catalog_cache(
        &self,
    ) -> Arc<crate::features::model_management::cache_adapter::ModelCacheAdapter> {
        Arc::clone(self.ai.model_catalog_cache())
    }

    // === LLM Use Case Getters (from AIModule) ===

    pub fn get_system_capabilities_use_case(&self) -> Arc<GetSystemCapabilitiesUseCase> {
        Arc::clone(self.ai.get_system_capabilities_use_case())
    }

    pub fn get_available_models_use_case(&self) -> Arc<GetAvailableModelsUseCase> {
        Arc::clone(self.ai.get_available_models_use_case())
    }

    pub fn get_recommended_models_use_case(&self) -> Arc<GetRecommendedModelsUseCase> {
        Arc::clone(self.ai.get_recommended_models_use_case())
    }

    pub fn get_best_model_use_case(&self) -> Arc<GetBestModelUseCase> {
        Arc::clone(self.ai.get_best_model_use_case())
    }

    pub fn check_model_downloaded_use_case(&self) -> Arc<CheckModelDownloadedUseCase> {
        Arc::clone(self.ai.check_model_downloaded_use_case())
    }

    pub fn get_model_path_use_case(&self) -> Arc<GetModelPathUseCase> {
        Arc::clone(self.ai.get_model_path_use_case())
    }

    pub fn download_model_use_case(&self) -> Arc<DownloadModelUseCase> {
        Arc::clone(self.ai.download_model_use_case())
    }

    pub fn delete_model_use_case(&self) -> Arc<DeleteModelUseCase> {
        Arc::clone(self.ai.delete_model_use_case())
    }

    pub fn list_models_use_case(&self) -> Arc<ListDownloadedModelsUseCase> {
        Arc::clone(self.ai.list_models_use_case())
    }

    // === Web Use Case Getters (from IndexingModule) ===

    pub fn ingest_web_url_use_case(&self) -> Arc<IngestWebUrlUseCase> {
        Arc::clone(self.indexing.ingest_web_url_use_case())
    }

    pub fn get_url_preview_use_case(&self) -> Arc<GetUrlPreviewUseCase> {
        Arc::clone(self.indexing.get_url_preview_use_case())
    }

    // === Batch Use Case Getters (from IndexingModule) ===

    pub fn start_batch_file_import_use_case(&self) -> Arc<StartBatchFileImportUseCase> {
        Arc::clone(self.indexing.start_batch_file_import_use_case())
    }

    pub fn start_batch_url_import_use_case(&self) -> Arc<StartBatchUrlImportUseCase> {
        Arc::clone(self.indexing.start_batch_url_import_use_case())
    }

    pub fn get_batch_job_status_use_case(&self) -> Arc<GetBatchJobStatusUseCase> {
        Arc::clone(self.indexing.get_batch_job_status_use_case())
    }

    pub fn cancel_batch_job_use_case(&self) -> Arc<CancelBatchJobUseCase> {
        Arc::clone(self.indexing.cancel_batch_job_use_case())
    }

    pub fn list_batch_jobs_use_case(&self) -> Arc<ListBatchJobsUseCase> {
        Arc::clone(self.indexing.list_batch_jobs_use_case())
    }

    pub fn delete_batch_job_use_case(&self) -> Arc<DeleteBatchJobUseCase> {
        Arc::clone(self.indexing.delete_batch_job_use_case())
    }

    pub fn retry_failed_items_use_case(&self) -> Arc<RetryFailedItemsUseCase> {
        Arc::clone(self.indexing.retry_failed_items_use_case())
    }

    // === Service Getters (for commands that use services directly, from IndexingModule) ===

    pub fn web_ingestion_service(&self) -> Arc<dyn WebIngestionServiceTrait> {
        Arc::clone(self.indexing.web_ingestion_service())
    }

    pub fn web_capture_service(&self) -> Arc<dyn WebCaptureServiceTrait> {
        Arc::clone(self.indexing.web_capture_service())
    }

    pub fn article_extractor_service(&self) -> Arc<dyn ArticleExtractorServiceTrait> {
        Arc::clone(self.indexing.article_extractor_service())
    }

    pub fn indexing_service(&self) -> Arc<dyn IndexingServiceTrait> {
        Arc::clone(self.indexing.indexing_service())
    }

    pub fn batch_file_import_service(&self) -> Arc<dyn BatchFileImportServiceTrait> {
        Arc::clone(self.indexing.batch_file_import_service())
    }

    pub fn batch_url_import_service(&self) -> Arc<dyn BatchUrlImportServiceTrait> {
        Arc::clone(self.indexing.batch_url_import_service())
    }

    pub fn batch_job_repository(&self) -> Arc<dyn BatchJobRepositoryPort> {
        Arc::clone(self.indexing.batch_job_repo())
    }

    // === Model Setup Support (for first-run commands) ===

    /// Get the models directory path
    ///
    /// Returns the path where AI models are stored (app_data_dir/models)
    /// This path is consistent with get_model_dir() in lib.rs
    pub fn models_path(&self) -> PathBuf {
        self.core.data_dir().join("models")
    }

    /// Recompute which directories file-read IPC is allowed to reach.
    ///
    /// The policy is: the app's own data directory, plus the user's vault,
    /// plus every folder they have asked us to index. Anything else in
    /// `$HOME` — SSH keys, cloud credentials, browser profiles — stays
    /// unreadable over IPC.
    ///
    /// Must be called at startup once settings are readable, and again after
    /// any change to the vault path or the indexed-folder list; otherwise a
    /// newly added folder is unreadable, or a removed one stays readable.
    pub async fn refresh_allowed_roots(&self) -> crate::shared::error::Result<()> {
        let settings = self.system.settings_repo().get_all().await?;

        let mut roots = vec![self.core.data_dir().to_path_buf()];

        if settings.vault.enabled && !settings.vault.vault_path.is_empty() {
            roots.push(PathBuf::from(&settings.vault.vault_path));
        }

        for indexed in &settings.indexing.indexed_paths {
            if !indexed.is_empty() {
                roots.push(PathBuf::from(indexed));
            }
        }

        // Folders being watched are also legitimately readable, and the watch
        // list is authoritative in the database rather than in settings.
        match sqlx::query_scalar::<_, String>("SELECT path FROM watch_folders")
            .fetch_all(self.db_pool())
            .await
        {
            Ok(paths) => roots.extend(paths.into_iter().map(PathBuf::from)),
            Err(e) => tracing::warn!(
                error = %e,
                "could not read watch folders while recomputing allowed roots"
            ),
        }

        let count = roots.len();
        self.file_access_config().set_allowed_roots(roots)?;
        tracing::info!(count, "recomputed file-access allowed roots");
        Ok(())
    }

    /// Get the backups directory path.
    ///
    /// Must stay in agreement with `BackupAdapter`, which derives the same
    /// location from the database path and refuses to *write* a backup
    /// anywhere else. Restore confines its *source* to this directory for the
    /// mirror-image reason: the database file it swaps in becomes the app's
    /// entire state, so accepting an arbitrary path lets a caller inject
    /// documents, settings and model rows wholesale.
    pub fn backups_path(&self) -> PathBuf {
        self.core.data_dir().join("backups")
    }

    /// Get the exports directory path.
    ///
    /// Destination for user-initiated exports. Confining writes here keeps
    /// export commands from doubling as an arbitrary-write primitive for the
    /// webview. If exporting to a user-chosen location is wanted, route it
    /// through a native save dialog so the *user*, not the renderer, picks
    /// the path.
    pub fn exports_path(&self) -> PathBuf {
        self.core.data_dir().join("exports")
    }

    /// Get download manager service
    ///
    /// Returns the shared download manager instance from AIModule.
    ///
    /// # Returns
    ///
    /// Arc<dyn DownloadManager> - The download manager service
    pub fn download_manager(
        &self,
    ) -> Result<Arc<dyn crate::features::download::manager::DownloadManager>> {
        Ok(Arc::clone(self.ai.download_manager()))
    }

    /// Get downloaded model repository (non-Arc version for use cases)
    ///
    /// Returns a repository for tracking downloaded models in the database.
    /// Creates a new instance each time (repository is lightweight).
    pub fn downloaded_model_repo_clone(
        &self,
    ) -> crate::infrastructure::persistence::repositories::DownloadedModelRepository {
        crate::infrastructure::persistence::repositories::DownloadedModelRepository::new(
            self.core.db_pool().clone(),
        )
    }
}

enum Outcome {
    Ready,
    Skipped,
    Failed(String),
}

/// First-run state (`AiModelsNotInstalled`) and router opt-out
/// (`InvalidConfig("...not configured...")`) are Skipped, not Failed —
/// the chat input mask shouldn't treat them as load errors.
fn classify_load_error(e: AppError) -> Outcome {
    match &e {
        AppError::AiModelsNotInstalled(_) => Outcome::Skipped,
        AppError::InvalidConfig(msg) if msg.contains("not configured") => Outcome::Skipped,
        AppError::ModelLoadFailed(msg) => Outcome::Failed(msg.clone()),
        _ => Outcome::Failed(e.to_string()),
    }
}

fn emit_warmup(app: &tauri::AppHandle, role: &str, phase: &str, error: Option<String>) {
    use tauri::Emitter;
    let payload = serde_json::json!({
        "role": role,
        "phase": phase,
        "error": error,
    });
    if let Err(e) = app.emit("model:warmup-status", payload) {
        tracing::warn!(role, phase, error = %e, "Failed to emit warmup status event");
    }
}
