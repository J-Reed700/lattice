//! DDD-Aligned Dependency Injection Container
//!
//! `Container` is the composition root. `Container::new` builds the seven
//! feature modules in `interfaces::di::modules` plus the runtime state that
//! outlives them: the chat/router/utility model caches, the embedding
//! runtime, the vault writer, and the Tauri `AppHandle`.
//!
//! Feature accessors do **not** live here. Each feature registers its own
//! surface on `Container` from an `impl Container` block in
//! `features::<feature>::di`, so a feature's wiring and the getters that
//! expose it sit in one file. Rust allows inherent impls anywhere in the
//! crate, so `State<'_, Container>` call sites in commands and plugins see
//! exactly the same API either way.
//!
//! What stays in this file: the struct, its construction, and the
//! cross-cutting core accessors (DB pool/connection, security context,
//! file-access config, and the data-dir-derived paths).

use crate::application::ports::{LLMPort, SettingsSideEffectsPort};
use crate::features::embedding::runtime::EmbeddingRuntime;
use crate::features::embedding::service::DynamicEmbeddingService;
use crate::features::embedding::EmbeddingServiceTrait;
use crate::features::function_calling::executor::{FunctionExecutor, FunctionExecutorServices};
use crate::features::function_calling::registry::{
    init_function_registry, register_custom_query_tools,
};
use crate::features::function_calling::{FunctionExecutorTrait, FunctionRegistryTrait};
use crate::features::web::services::web::WebService;
use crate::infrastructure::ml::model_cache::ModelCache;
use crate::infrastructure::security::{FileAccessConfig, SecurityContext};
use crate::infrastructure::storage::ContentAddressedStorage;
use crate::interfaces::settings_side_effects::ContainerSettingsSideEffects;
use crate::shared::error::Result;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Named-model cache (router / utility roles), keyed by model name.
pub(crate) type NamedLlmCache = Arc<ModelCache<String, Arc<dyn LLMPort>>>;

/// Single-slot chat model cache.
pub(crate) type ChatLlmCache = Arc<ModelCache<(), Arc<dyn LLMPort>>>;

/// Composition root for the application's dependency graph.
///
/// Holds seven feature modules (see `interfaces::di::modules`) instead of one
/// field per service: each module constructor gets and releases its own stack
/// frame, which is what keeps boot inside the 2MB thread stack.
///
/// Commands can use either:
/// - `State<'_, Container>` (the common case; feature registrars supply the
///   getters, see the module docs above)
/// - `State<'_, SearchModule>` for direct module access
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
    pub(crate) function_registry: Arc<dyn FunctionRegistryTrait>,

    /// Function executor for LLM tools
    pub(crate) function_executor: Arc<dyn FunctionExecutorTrait>,

    /// The one web service, held as itself rather than as its trait so the
    /// reader command can ask for a page and be told when the text was
    /// actually read — something the model's tool output has no field for.
    pub(crate) web_service: Arc<WebService>,

    /// Router LLM cache (optional smaller routing model)
    pub(crate) router_llm_cache: NamedLlmCache,

    /// Utility LLM cache (HyDE expansion, intent routing, follow-up)
    pub(crate) utility_llm_cache: NamedLlmCache,

    /// Chat cache owns single-flight loading and invalidation-safe publication.
    pub(crate) llm_cache: ChatLlmCache,
    /// Owns embedding readiness checks, loading, and retry cooldown.
    pub(crate) embedding_runtime: Arc<EmbeddingRuntime>,

    /// All vault writes flow through a single mpsc-fed worker; see
    /// `features::vault::writeback`.
    pub(crate) vault_writer: crate::features::vault::writeback::VaultWriterHandle,

    /// Shared by writer + watcher for loop suppression.
    pub(crate) vault_write_suppression: crate::features::vault::watcher::WriteSuppressionRegistry,

    /// Settings side-effects port (audit P0-3 fix). Built post-AI-module
    /// so it has access to the LLM cache + router LLM cache + function
    /// executor. Injected into freshly-constructed
    /// `UpdateSettingsUseCase` / `ResetSettingsUseCase` instances on
    /// every `Container::*_settings_use_case()` call so internal
    /// callers (tests, watch-folder helpers, future migration scripts)
    /// trigger cache invalidation just like the Tauri command path
    /// did manually before.
    pub(crate) settings_side_effects: Arc<dyn SettingsSideEffectsPort>,

    /// Durable background jobs. Features register a handler per kind during
    /// plugin setup; shutdown closes and drains it before the database closes.
    pub(crate) jobs: Arc<crate::shared::runtime::jobs::JobRuntime>,

    /// Frozen copies of cited web pages; its captures run on `jobs`.
    pub(crate) web_page_snapshots: Arc<crate::features::web::snapshots::WebPageSnapshotService>,
    /// Where the snapshot service sends its events; attached to the main
    /// window in [`Container::with_app_handle`].
    web_snapshot_events: Arc<crate::features::web::snapshots::AppEvents>,

    /// The Explorer's folder indexes: the open folder and its builds, which
    /// run as jobs. Built with the app handle, which it loads the embedder
    /// through; `None` in test fixtures.
    pub(crate) folder_index:
        Option<Arc<crate::features::explorer::index::manager::FolderIndexManager>>,

    /// The one registry of in-flight turn stop buttons for this process: a
    /// stop press and the turn it targets must meet in the same map.
    pub(crate) turn_cancellation:
        Arc<crate::features::conversation::chat::cancellation::TurnCancellationRegistry>,

    /// The one map of commands waiting on their deep research turns for this
    /// process: a research job and the command it answers must meet in it.
    pub(crate) research_waiters: Arc<crate::features::conversation::chat::ResearchWaiters>,

    /// The one registry of in-flight learning outline runs for this process.
    pub(crate) outline_run_registry:
        Arc<crate::features::learning::outline_progress::OutlineRunRegistry>,

    /// The one registry of learning outline drafts being reviewed, repaired
    /// or accepted in this process.
    pub(crate) draft_run_registry: Arc<crate::features::learning::outline_draft::DraftRunRegistry>,

    /// The one registry of executing learning practical runs for this process.
    pub(crate) practical_run_registry:
        Arc<crate::features::learning::practical_runs::PracticalRunRegistry>,

    /// The one search result cache: the search commands read it and every
    /// indexer invalidates it, so all of them must reach the same instance.
    pub(crate) query_cache: Arc<crate::features::search::query_cache::QueryCache>,

    /// The summary tier's trigger and open indexes. Indexing reports to the
    /// trigger; registration installs and clears its hook.
    pub(crate) summary_tier: Arc<crate::features::summaries::di::SummaryTier>,

    /// What every model load shares: the per-backend schedulers, the
    /// artifacts known not to start, and the sidecar's device probe.
    pub(crate) llm_runtime: crate::features::llm::loading::LlmRuntime,

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
    pub(crate) app_handle: Option<tauri::AppHandle>,
}

impl Container {
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
        _embedding_model_path: Option<String>,
        llm_endpoint: &str,
        llm_model: &str,
        data_dir: PathBuf,
    ) -> Result<Self> {
        // These are populated asynchronously when models load
        let embedding_runtime = Arc::new(EmbeddingRuntime::new());
        let llm_cache: ChatLlmCache = Arc::new(ModelCache::new());
        let router_llm_cache = Arc::new(ModelCache::new());
        let utility_llm_cache = Arc::new(ModelCache::new());
        tracing::info!("Building Container with modular architecture (Hollow Container Pattern)");

        // State more than one feature reaches. Each is built once, here, and
        // handed to whatever uses it, so every user shares the one instance.
        let query_cache = Arc::new(crate::features::search::query_cache::QueryCache::for_search());
        let summary_tier = Arc::new(crate::features::summaries::di::SummaryTier::new());
        let extraction_workers =
            crate::features::indexing::engine::extraction::ExtractionWorkers::new();
        let backend_devices = Arc::new(crate::features::llm::engine::system::BackendDevices::new());
        let llm_runtime =
            crate::features::llm::loading::LlmRuntime::new(Arc::clone(&backend_devices));

        // CoreModule builds shared singletons (security, runtime, credentials)
        // Stack frame for this construction is freed before Layer 2 begins
        let core = Arc::new(
            super::modules::CoreModule::new(db_pool.clone(), db_conn.clone(), data_dir.clone())
                .await?,
        );

        tracing::debug!("CoreModule initialized");

        // Each module builds its own dependencies. Stack frames are independent.

        let system = Arc::new(
            super::modules::SystemModule::new(
                db_pool.clone(),
                core.clone(),
                Arc::clone(&backend_devices),
            )
            .await?,
        );
        tracing::debug!("SystemModule initialized");

        let search = Arc::new(
            super::modules::SearchModule::new(
                db_pool.clone(),
                core.clone(),
                embedding_runtime.clone(),
            )
            .await?,
        );
        tracing::debug!("SearchModule initialized");

        let library =
            Arc::new(super::modules::LibraryModule::new(db_pool.clone(), core.clone()).await?);
        tracing::debug!("LibraryModule initialized");

        let file_ops = Arc::new(
            super::modules::FileOpsModule::new(
                db_pool.clone(),
                core.clone(),
                extraction_workers.clone(),
            )
            .await?,
        );
        tracing::debug!("FileOpsModule initialized");

        let ai = Arc::new(
            super::modules::AIModule::new(
                db_pool.clone(),
                core.clone(),
                llm_cache.clone(),
                llm_endpoint,
                llm_model,
                backend_devices,
            )
            .await?,
        );
        tracing::debug!("AIModule initialized");

        let indexing = Arc::new(
            super::modules::IndexingModule::new(
                db_pool,
                core.clone(),
                embedding_runtime.clone(),
                search.vector_search().clone(),
                crate::features::indexing::di::IndexingShared {
                    query_cache: query_cache.clone(),
                    index_observer: summary_tier.trigger().clone(),
                    extraction_workers,
                },
            )
            .await?,
        );
        tracing::debug!("IndexingModule initialized");

        tracing::info!("Container fully initialized with 7 modules");

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

        let embedding_service = Arc::new(DynamicEmbeddingService::new(embedding_runtime.clone()))
            as Arc<dyn EmbeddingServiceTrait>;
        // One service for the whole process: its page cache and its per-site
        // request queue are only worth anything if every caller shares them.
        let web_service = Arc::new(WebService::new(core.data_dir())?);

        let function_executor = Arc::new(FunctionExecutor::new_with_custom_tools(
            FunctionExecutorServices {
                registry: function_registry.clone(),
                embedding_service,
                search_service: search.search_service().clone(),
                bm25_service: search.bm25_search_service().clone(),
                hybrid_search: search.hybrid_search_use_case().clone(),
                document_repository: search.document_repo().clone(),
                chunk_repository: indexing.chunk_repository().clone(),
                tag_service: library.tag_service().clone(),
                favorites_repository: library.favorites_repo().clone(),
                recent_documents_repository: library.recent_docs_repo().clone(),
                file_storage: indexing.file_storage().clone(),
                web_service: web_service.clone(),
            },
            custom_tool_map,
        )) as Arc<dyn FunctionExecutorTrait>;

        let vault_write_suppression =
            crate::features::vault::watcher::WriteSuppressionRegistry::new();
        let vault_writer = crate::features::vault::writeback::start_vault_writer(
            Arc::clone(system.get_settings_use_case()),
            vault_write_suppression.clone(),
            core.db_pool().clone(),
        );

        let settings_side_effects: Arc<dyn SettingsSideEffectsPort> =
            Arc::new(ContainerSettingsSideEffects {
                llm_cache: llm_cache.clone(),
                router_llm_cache: router_llm_cache.clone(),
                utility_llm_cache: utility_llm_cache.clone(),
                function_executor: function_executor.clone(),
                settings_repository: system.settings_repo().clone(),
                db_pool: core.db_pool().clone(),
                vault_writer: vault_writer.clone(),
            });

        let jobs = crate::shared::runtime::jobs::JobRuntime::new(core.db_pool().clone());
        let web_snapshot_events = Arc::new(crate::features::web::snapshots::AppEvents::default());
        let web_page_snapshots = crate::features::web::di::build_page_snapshots(
            core.db_pool().clone(),
            Arc::clone(&jobs),
            Arc::clone(&web_snapshot_events),
        )?;

        let turn_cancellation = Arc::new(
            crate::features::conversation::chat::cancellation::TurnCancellationRegistry::new(),
        );
        let research_waiters =
            Arc::new(crate::features::conversation::chat::ResearchWaiters::new());
        let outline_run_registry =
            Arc::new(crate::features::learning::outline_progress::OutlineRunRegistry::new());
        let draft_run_registry =
            Arc::new(crate::features::learning::outline_draft::DraftRunRegistry::new());
        let practical_run_registry =
            Arc::new(crate::features::learning::practical_runs::PracticalRunRegistry::new());

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
            web_service,
            router_llm_cache,
            utility_llm_cache,
            llm_cache,
            embedding_runtime,
            vault_writer,
            vault_write_suppression,
            settings_side_effects,
            jobs,
            web_page_snapshots,
            web_snapshot_events,
            turn_cancellation,
            research_waiters,
            outline_run_registry,
            draft_run_registry,
            practical_run_registry,
            query_cache,
            summary_tier,
            llm_runtime,
            folder_index: None,
            app_handle: None,
        })
    }

    /// Attach the Tauri `AppHandle` post-construction. Real callers chain
    /// `.with_app_handle(handle)`; tests skip it.
    pub fn with_app_handle(mut self, handle: tauri::AppHandle) -> Self {
        self.vault_writer.set_app_handle(handle.clone());
        self.web_snapshot_events.attach(handle.clone());
        // A page that refuses the HTTP client is read in a hidden window of
        // the app's own browser engine, which exists only from here on.
        self.web_service.attach_browser(Arc::new(
            crate::features::web::services::browser_reader::BrowserReader::new(handle.clone()),
        ));
        // The vault writer starts before the container exists, so its corpus
        // sink is installed here, once the indexing pipeline is built. Until
        // then — and in tests that skip this step — the writer defers its
        // outbox entries and retries on the next pass. The sink gets only the
        // pieces it uses, never the container: the container owns the writer,
        // so a container handle inside the sink would be a reference cycle
        // that keeps the writer's lifetime guard from ever dropping.
        use crate::features::indexing::note_index_sink::NoteIndexSinkAdapter;
        let note_index_sink = NoteIndexSinkAdapter::new(
            self.index_file_use_case(),
            self.delete_document_use_case(),
            Arc::clone(self.indexing.document_repo()),
            Arc::new(self.active_embedding()),
            self.get_settings_use_case(),
            self.file_access_config(),
            Arc::clone(self.indexing.indexing_state()),
        );
        self.vault_writer
            .set_note_index_sink(Arc::new(note_index_sink));
        // Watcher reads settings once and exits if disabled.
        crate::features::vault::watcher::start_vault_watcher(
            Arc::clone(self.system.get_settings_use_case()),
            self.core.db_pool().clone(),
            handle.clone(),
            self.vault_write_suppression.clone(),
        );

        let events = handle.clone();
        self.jobs.observe(move |job| {
            use tauri::Emitter;
            if let Err(error) = events.emit(crate::shared::runtime::jobs::STATUS_EVENT, job) {
                tracing::debug!(%error, "Could not publish a job status to the window");
            }
        });

        self.folder_index = Some(
            crate::features::explorer::index::manager::FolderIndexManager::new(
                crate::features::explorer::index::manager::ManagerConfig::for_data_dir(
                    self.core.data_dir(),
                ),
                Arc::new(crate::features::explorer::plugin::AppEmbedders::new(
                    handle.clone(),
                )),
                Arc::clone(&self.jobs),
            ),
        );

        self.app_handle = Some(handle);
        self
    }

    pub fn db_pool(&self) -> &SqlitePool {
        self.core.db_pool()
    }

    pub fn jobs(&self) -> &Arc<crate::shared::runtime::jobs::JobRuntime> {
        &self.jobs
    }

    /// The one registry of in-flight learning outline runs: the commands
    /// that start or stop a run all consult this same map.
    pub fn outline_run_registry(
        &self,
    ) -> Arc<crate::features::learning::outline_progress::OutlineRunRegistry> {
        Arc::clone(&self.outline_run_registry)
    }

    /// The one registry of learning outline drafts in use: generation,
    /// repair and acceptance of a draft all consult this same set.
    pub fn draft_run_registry(
        &self,
    ) -> Arc<crate::features::learning::outline_draft::DraftRunRegistry> {
        Arc::clone(&self.draft_run_registry)
    }

    /// The one registry of executing learning practical runs: the commands
    /// that start or cancel a run all consult this same map.
    pub fn practical_run_registry(
        &self,
    ) -> Arc<crate::features::learning::practical_runs::PracticalRunRegistry> {
        Arc::clone(&self.practical_run_registry)
    }

    pub fn folder_index(
        &self,
    ) -> Option<&Arc<crate::features::explorer::index::manager::FolderIndexManager>> {
        self.folder_index.as_ref()
    }

    pub fn db_conn(
        &self,
    ) -> &Arc<crate::infrastructure::persistence::database::DatabaseConnection> {
        self.core.db_conn()
    }

    pub fn security_context(&self) -> &Arc<SecurityContext> {
        self.core.security_context()
    }

    /// Get file access configuration (for secure path validation)
    ///
    /// Returns Arc-wrapped FileAccessConfig for use by file operation use cases.
    /// This enforces path validation against allowed_roots (lattice + indexed directories).
    pub fn file_access_config(&self) -> Arc<FileAccessConfig> {
        Arc::clone(self.core.file_access_config())
    }

    /// Get the models directory path
    ///
    /// Returns the path where AI models are stored (app_data_dir/models)
    /// This path is consistent with get_model_dir() in lib.rs
    pub fn models_path(&self) -> PathBuf {
        self.core.data_dir().join("models")
    }

    /// The manager that fetches and inspects the models kept under
    /// [`Self::models_path`].
    pub fn model_manager(
        &self,
    ) -> Arc<dyn crate::infrastructure::services::traits::ModelManagerTrait> {
        Arc::clone(self.system.model_manager())
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

    /// Recompute which directories file-read IPC is allowed to reach.
    ///
    /// The policy is: the app's own data and imported-file library directories,
    /// plus the user's vault and every folder they have asked us to index.
    /// Anything else in `$HOME` — SSH keys, cloud credentials, browser profiles — stays
    /// unreadable over IPC.
    ///
    /// Must be called at startup once settings are readable, and again after
    /// any change to the vault path or the indexed-folder list; otherwise a
    /// newly added folder is unreadable, or a removed one stays readable.
    pub async fn refresh_allowed_roots(&self) -> crate::shared::error::Result<()> {
        let settings = self.system.settings_repo().get_all().await?;

        let mut roots = vec![
            self.core.data_dir().to_path_buf(),
            ContentAddressedStorage::default_library_root()?,
        ];

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

        // The viewers load images and audio over `asset://`, whose static
        // scope is only the app data and the library; the user's own folders
        // are opened to it here, as to IPC. Tauri cannot take a directory back
        // out of the scope, so a removed folder stays viewable until restart.
        if let Some(handle) = &self.app_handle {
            use tauri::Manager;
            let scope = handle.asset_protocol_scope();
            for root in &roots {
                scope.allow_directory(root, true).map_err(|e| {
                    crate::shared::error::AppError::InternalError(format!(
                        "could not open {} to the asset protocol: {e}",
                        root.display()
                    ))
                })?;
            }
        }

        let count = roots.len();
        self.file_access_config().set_allowed_roots(roots)?;
        tracing::info!(count, "recomputed file-access allowed roots");
        Ok(())
    }
}

/// The note-index sink's readiness gate, implemented over the embedding
/// runtime and its loader: the same single-flight load with retry cooldown
/// that `get_or_load_embedding` gives every other caller, then the shared
/// readiness check the `index_file` command applies. Cross-feature — the
/// port is indexing's, the runtime is embedding's — so it is bound here,
/// where both may be named.
#[async_trait::async_trait]
impl crate::features::indexing::note_index_sink::EmbeddingReadiness
    for crate::features::embedding::runtime::ActiveEmbedding
{
    async fn ensure_ready(&self) -> Result<()> {
        crate::features::indexing::commands::require_embedding_ready(self.get_or_load().await).await
    }
}

/// The note-index sink's chunk-size preference, implemented over the
/// settings use case. The impl sits at the composition root because it
/// crosses two features — indexing's port, satisfied from settings — and
/// the root is the one place allowed to name both.
#[async_trait::async_trait]
impl crate::features::indexing::note_index_sink::ChunkSizePreference
    for crate::features::settings::use_cases::GetSettingsUseCase
{
    async fn chunk_size(&self) -> Result<u32> {
        self.execute()
            .await
            .map(|settings| settings.indexing.chunk_size)
    }
}

/// The reranker setup's toggle, implemented over the settings use case, for
/// the same reason as the chunk-size preference: search's port, satisfied
/// from settings, bound where both may be named.
#[async_trait::async_trait]
impl crate::features::search::reranker_setup::RerankingToggle for Container {
    async fn enabled(&self) -> Result<bool> {
        self.get_settings_use_case()
            .execute()
            .await
            .map(|settings| settings.search.enable_reranking)
    }
}

/// The journal synthesis's entry reads, implemented over the references and
/// daily-notes repositories. The impl sits at the composition root for the
/// same reason as its siblings: conversation's port, satisfied from two
/// other features, bound where all three may be named.
#[async_trait::async_trait]
impl crate::features::conversation::chat::ports::SynthesisEntries for Container {
    async fn references_created_since(
        &self,
        cutoff: &str,
        limit: i64,
    ) -> crate::shared::error::Result<
        Vec<crate::features::references::repository::PassageReferenceRecord>,
    > {
        crate::features::references::repository::PassageReferenceRepository::new(
            self.db_pool().clone(),
        )
        .list_created_since(cutoff, limit)
        .await
    }

    async fn notes_in_range(
        &self,
        start_date: &str,
        end_date: &str,
    ) -> crate::shared::error::Result<
        Vec<crate::features::conversation::chat::ports::StoredJournalPage>,
    > {
        let rows = crate::features::daily_notes::repository::DailyNotesRepository::new(
            self.db_pool().clone(),
        )
        .list_in_date_range(start_date, end_date)
        .await?;
        // Field for field, so a column added to the row fails to compile here.
        Ok(rows
            .into_iter()
            .map(|row| {
                let crate::features::daily_notes::repository::WorkspaceNoteRecord {
                    revision,
                    id,
                    title,
                    journal_id,
                    content,
                    linked_document_ids,
                    linked_conversation_ids,
                    highlights_json,
                    sticky_notes_json,
                    conversation_snapshots_json,
                    sources_json,
                    created_at,
                    updated_at,
                } = row;
                crate::features::conversation::chat::ports::StoredJournalPage {
                    revision,
                    id,
                    title,
                    journal_id,
                    content,
                    linked_document_ids,
                    linked_conversation_ids,
                    highlights_json,
                    sticky_notes_json,
                    conversation_snapshots_json,
                    sources_json,
                    created_at,
                    updated_at,
                }
            })
            .collect())
    }

    async fn hydrate_note(
        &self,
        page: crate::features::conversation::chat::ports::StoredJournalPage,
    ) -> crate::shared::error::Result<crate::features::daily_notes::dto::WorkspaceNoteDto> {
        let crate::features::conversation::chat::ports::StoredJournalPage {
            revision,
            id,
            title,
            journal_id,
            content,
            linked_document_ids,
            linked_conversation_ids,
            highlights_json,
            sticky_notes_json,
            conversation_snapshots_json,
            sources_json,
            created_at,
            updated_at,
        } = page;
        let row = crate::features::daily_notes::repository::WorkspaceNoteRecord {
            revision,
            id,
            title,
            journal_id,
            content,
            linked_document_ids,
            linked_conversation_ids,
            highlights_json,
            sticky_notes_json,
            conversation_snapshots_json,
            sources_json,
            created_at,
            updated_at,
        };
        crate::features::daily_notes::commands::hydrate_workspace_note(
            &crate::features::daily_notes::repository::DailyNotesRepository::new(
                self.db_pool().clone(),
            ),
            self,
            row,
        )
        .await
    }
}

/// The conversation archive a journal page recovers its web citations from,
/// read through the conversation repository. Daily notes is the record and
/// conversations are built on it, so the port is daily notes' and is bound
/// here, where both may be named.
#[async_trait::async_trait]
impl crate::features::daily_notes::ports::LinkedConversationArchive for Container {
    async fn archived_web_page(
        &self,
        conversation_id: &str,
        url: &str,
    ) -> Result<Option<crate::features::daily_notes::ports::ArchivedWebPage>> {
        let snapshot = crate::features::conversation::repository::ConversationRepository::new(
            self.db_pool().clone(),
        )
        .conversation_web_source_snapshot(conversation_id.to_string(), url.to_string())
        .await?;
        // Field for field, so a field added to the archive fails to compile here.
        Ok(snapshot.map(|snapshot| {
            let crate::features::conversation::workspace_dto::ConversationWebSourceSnapshotDto {
                title,
                content,
                fetched_at,
                truncated,
            } = snapshot;
            crate::features::daily_notes::ports::ArchivedWebPage {
                title,
                content,
                fetched_at,
                truncated,
            }
        }))
    }
}

/// The vault's markdown mirror as daily notes' projection. The vault is a
/// projection of the journal onto files, so it serves daily notes' port
/// rather than daily notes calling into the vault; the binding sits here
/// because the container owns the writer.
impl crate::features::daily_notes::ports::NoteProjection for Container {
    fn note_saved(&self) {
        crate::features::vault::writeback::spawn_sync_workspace_note(self.vault_writer());
    }

    fn note_deleted(&self) {
        crate::features::vault::writeback::spawn_delete_workspace_note(self.vault_writer());
    }
}

/// Explorer's folder threads, deleted through the chat's conversation delete.
/// Conversations are built on Explorer, so the port is Explorer's and is bound
/// here, where both may be named.
#[async_trait::async_trait]
impl crate::features::explorer::folders::FolderThreads for Container {
    async fn check_delete_limit(&self) -> Result<()> {
        crate::features::conversation::chat::ports::ChatPolicy::check_delete_rate_limit(
            self,
            "delete_conversation",
        )
        .await
    }

    async fn delete_thread(&self, conversation_id: &str) -> Result<()> {
        crate::features::conversation::chat::ports::ChatPolicy::delete_conversation_unmetered(
            self,
            conversation_id,
        )
        .await
    }
}
