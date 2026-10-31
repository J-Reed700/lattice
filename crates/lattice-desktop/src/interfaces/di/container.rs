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
//! What stays in this module: the struct, its construction, and the
//! cross-cutting core accessors (DB pool/connection, security context,
//! file-access config, and the data-dir-derived paths).

use crate::features::embedding::runtime::EmbeddingRuntime;
use crate::features::embedding::service::DynamicEmbedding as DynamicEmbeddingService;
use crate::features::function_calling::executor::{FunctionExecutor, FunctionExecutorServices};
use crate::features::function_calling::registry::{
    init_function_registry, register_custom_query_tools,
};
use crate::features::function_calling::{FunctionExecutorTrait, FunctionRegistryTrait};
use crate::features::llm::instances::LlmInstances;
use crate::features::web::services::web::WebService;
use crate::interfaces::settings_side_effects::ContainerSettingsSideEffects;
use lattice_core::application::ports::EmbeddingPort as EmbeddingServiceTrait;
use lattice_core::application::ports::SettingsSideEffectsPort;
use lattice_core::shared::error::Result;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

mod accessors;
mod activity;
mod port_bindings;

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

    /// Every LLM loaded now, for every role: chats, the Learning model, the
    /// router and the utility model.
    pub(crate) llm_instances: Arc<LlmInstances>,
    /// Owns embedding readiness checks, loading, and retry cooldown.
    pub(crate) embedding_runtime: Arc<EmbeddingRuntime>,
    /// Starts the llama-server the embedding model runs on.
    pub(crate) embedding_starter:
        Arc<dyn crate::features::embedding::llama_cpp::EmbeddingServerStarter>,
    /// The embedding servers `embedding_starter` has started, for the
    /// activity view.
    pub(crate) embedding_servers: Arc<crate::features::embedding::activity::StartedServers>,

    /// All vault writes flow through a single mpsc-fed worker; see
    /// `features::vault::writeback`.
    pub(crate) vault_writer: crate::features::vault::writeback::VaultWriterHandle,

    /// Shared by writer + watcher for loop suppression.
    pub(crate) vault_write_suppression: crate::features::vault::watcher::WriteSuppressionRegistry,

    /// Shared by the watcher and the rescan command, so full rescans never overlap.
    pub(crate) vault_rescan_lock: crate::features::vault::watcher::RescanLock,

    /// Settings side-effects port (audit P0-3 fix). Built post-AI-module
    /// so it has access to the loaded LLM instances and the function
    /// executor. Injected into freshly-constructed
    /// `UpdateSettingsUseCase` / `ResetSettingsUseCase` instances on
    /// every `Container::*_settings_use_case()` call so internal
    /// callers (tests, watch-folder helpers, future migration scripts)
    /// invalidate stale models just like the Tauri command path.
    pub(crate) settings_side_effects: Arc<dyn SettingsSideEffectsPort>,

    /// Durable background jobs. Features register a handler per kind during
    /// plugin setup; shutdown closes and drains it before the database closes.
    pub(crate) jobs: Arc<lattice_core::shared::runtime::jobs::JobRuntime>,

    /// Frozen copies of cited web pages; its captures run on `jobs`.
    pub(crate) web_page_snapshots: Arc<crate::features::web::snapshots::WebPageSnapshotService>,
    /// Where the snapshot service sends its events; attached to the main
    /// window in [`Container::with_app_handle`].
    web_snapshot_events: Arc<crate::features::web::snapshots::AppEvents>,

    /// Pictures shown inline in chat answers; fetched through `web_service`.
    pub(crate) chat_images: Arc<crate::features::web::images::ChatImageService>,

    /// The Explorer's folder indexes: the open folder and its builds, which
    /// run as jobs. Built with the app handle, which it loads the embedder
    /// through; `None` in test fixtures.
    pub(crate) folder_index:
        Option<Arc<crate::features::explorer::index::manager::FolderIndexManager>>,

    /// Every course's evidence index and its builds, which run as jobs.
    /// Built with the app handle beside `folder_index`, sharing its embedder
    /// source and search factory; `None` in test fixtures.
    pub(crate) course_indexes:
        Option<Arc<crate::features::learning::references::course_index::CourseIndexes>>,

    /// The turn order the folder and course indexes embed in. Built with
    /// them; `None` in test fixtures.
    pub(crate) index_turns: Option<Arc<crate::infrastructure::passage_index::EmbedTurns>>,

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
        app_handle: Option<tauri::AppHandle>,
        llm_endpoint: &str,
        llm_model: &str,
        data_dir: PathBuf,
        background: Arc<lattice_core::shared::runtime::background::BackgroundTasks>,
    ) -> Result<Self> {
        // These are populated asynchronously when models load
        let embedding_runtime = Arc::new(EmbeddingRuntime::new());
        // The embedding and reranking servers are the servers outside the pool
        // of LLMs that their admission may stop while they are idle.
        let current_embedding = Arc::new(super::modules::current_server::CurrentServer::new(
            "embedding server",
        ));
        let current_reranker = Arc::new(super::modules::current_server::CurrentServer::new(
            "reranking server",
        ));
        let llm_instances = Arc::new(
            LlmInstances::new()
                .with_evictable(current_embedding.clone())
                .with_evictable(current_reranker.clone()),
        );
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
        // Needs the app before the search module opens: its backfill starts
        // the embedding server.
        let embedding_servers =
            Arc::new(crate::features::embedding::activity::StartedServers::default());
        let rerank_starter: Arc<
            dyn crate::features::search::engine::llama_reranker::RerankServerStarter,
        > = Arc::new(super::modules::reranking::SidecarRerankStarter::new(
            app_handle.clone(),
            llm_instances.clone(),
            llm_runtime.clone(),
            current_reranker,
        ));
        let embedding_starter: Arc<
            dyn crate::features::embedding::llama_cpp::EmbeddingServerStarter,
        > = Arc::new(crate::features::embedding::activity::WatchedStarter::new(
            Arc::new(super::modules::embedding::SidecarEmbeddingStarter::new(
                app_handle,
                llm_instances.clone(),
                llm_runtime.clone(),
                current_embedding,
            )),
            Arc::clone(&embedding_servers),
        ));

        // CoreModule builds shared singletons (security, runtime, credentials)
        // Stack frame for this construction is freed before Layer 2 begins
        let core = Arc::new(
            super::modules::CoreModule::new(
                db_pool.clone(),
                db_conn.clone(),
                data_dir.clone(),
                background,
            )
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
                embedding_starter.clone(),
                rerank_starter,
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
                llm_instances.clone(),
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
            core.background(),
        );

        let settings_side_effects: Arc<dyn SettingsSideEffectsPort> =
            Arc::new(ContainerSettingsSideEffects {
                llm_instances: llm_instances.clone(),
                function_executor: function_executor.clone(),
                settings_repository: system.settings_repo().clone(),
                db_pool: core.db_pool().clone(),
                vault_writer: vault_writer.clone(),
                web_service: Arc::clone(&web_service),
                credentials: Arc::clone(core.credentials()),
            });

        let jobs = lattice_core::shared::runtime::jobs::JobRuntime::new(core.db_pool().clone());
        let web_snapshot_events = Arc::new(crate::features::web::snapshots::AppEvents::default());
        let web_page_snapshots = crate::features::web::di::build_page_snapshots(
            core.db_pool().clone(),
            Arc::clone(&jobs),
            Arc::clone(&web_snapshot_events),
            Arc::clone(core.background()),
        )?;
        let chat_images = crate::features::web::di::build_chat_images(
            core.db_pool().clone(),
            Arc::clone(&web_service),
            Arc::clone(core.background()),
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
            llm_instances,
            embedding_runtime,
            embedding_starter,
            embedding_servers,
            vault_writer,
            vault_write_suppression,
            vault_rescan_lock: Default::default(),
            settings_side_effects,
            jobs,
            web_page_snapshots,
            web_snapshot_events,
            chat_images,
            turn_cancellation,
            research_waiters,
            outline_run_registry,
            draft_run_registry,
            practical_run_registry,
            query_cache,
            summary_tier,
            llm_runtime,
            folder_index: None,
            course_indexes: None,
            index_turns: None,
            app_handle: None,
        })
    }

    /// Serve embeddings through `starter` instead of the bundled sidecar.
    /// Tools that build a container without the app (the shipped-path eval)
    /// pass a [`StandaloneServers`](super::modules::standalone::StandaloneServers).
    /// After construction the container is the starter's only holder, so
    /// every load and activation from here on uses this one.
    pub fn with_embedding_starter(
        mut self,
        starter: Arc<dyn crate::features::embedding::llama_cpp::EmbeddingServerStarter>,
    ) -> Self {
        self.embedding_starter =
            Arc::new(crate::features::embedding::activity::WatchedStarter::new(
                starter,
                Arc::clone(&self.embedding_servers),
            ));
        self
    }

    /// Serve the reranker in `gguf` through `starter` instead of the bundled
    /// sidecar, as the evals that build a container without the app do.
    pub async fn with_reranker(
        self,
        gguf: impl Into<PathBuf>,
        starter: Arc<dyn crate::features::search::engine::llama_reranker::RerankServerStarter>,
    ) -> Self {
        self.search.reranker().serve_from(gguf, starter).await;
        self
    }

    /// Attach the Tauri `AppHandle` post-construction. Real callers chain
    /// `.with_app_handle(handle)`; tests skip it.
    pub fn with_app_handle(mut self, handle: tauri::AppHandle) -> Self {
        self.vault_writer.set_app_handle(handle.clone());
        self.web_snapshot_events.attach(handle.clone());
        // A page that refuses the HTTP client is read in a hidden window of
        // the app's own browser engine, which exists only from here on.
        let browser = Arc::new(
            crate::features::web::services::browser_reader::BrowserReader::new(handle.clone()),
        );
        self.web_service.attach_browser(Arc::clone(&browser));
        self.web_page_snapshots.attach_browser(browser);
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
            Arc::clone(self.core.background()),
            self.vault_rescan_lock.clone(),
        );

        let events = handle.clone();
        self.jobs.observe(move |job| {
            use tauri::Emitter;
            if let Err(error) = events.emit(lattice_core::shared::runtime::jobs::STATUS_EVENT, job)
            {
                tracing::debug!(%error, "Could not publish a job status to the window");
            }
        });

        let embedders: Arc<dyn crate::infrastructure::passage_index::EmbedderSource> = Arc::new(
            crate::features::embedding::di::AppEmbedders::new(handle.clone()),
        );
        let searches: Arc<
            dyn lattice_core::application::ports::hybrid_search::HybridSearchFactory,
        > = Arc::new(crate::features::search::trait_def::HybridSearchUseCaseFactory);
        // One turn order for every index's embedding, so a course someone
        // waits on goes ahead of folder builds as well as other courses'.
        let turns: Arc<crate::infrastructure::passage_index::EmbedTurns> = Arc::default();
        self.folder_index = Some(
            crate::features::explorer::index::manager::FolderIndexManager::new(
                crate::features::explorer::index::manager::ManagerConfig::for_data_dir(
                    self.core.data_dir(),
                ),
                Arc::clone(&embedders),
                Arc::clone(&searches),
                Arc::clone(&self.jobs),
                Arc::clone(self.core.background()),
                Arc::clone(&turns),
            ),
        );
        self.course_indexes = Some(
            crate::features::learning::references::course_index::CourseIndexes::new(
                self.core.data_dir().join("course-index"),
                self.core.db_pool().clone(),
                embedders,
                searches,
                Arc::clone(&self.jobs),
                Arc::clone(self.core.background()),
                Arc::clone(&turns),
            ),
        );
        self.index_turns = Some(turns);

        self.app_handle = Some(handle);
        self
    }
}
