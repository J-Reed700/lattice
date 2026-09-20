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
use crate::features::embedding::service::DynamicEmbeddingService;
use crate::features::embedding::EmbeddingServiceTrait;
use crate::features::function_calling::{FunctionExecutorTrait, FunctionRegistryTrait};
use crate::features::settings::di::ContainerSettingsSideEffects;
use crate::infrastructure::security::{FileAccessConfig, SecurityContext};
use crate::infrastructure::services::{
    init_function_registry, register_custom_query_tools, FunctionExecutor, WebService,
};
use crate::infrastructure::storage::ContentAddressedStorage;
use crate::shared::error::Result;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Named-model cache (router / utility roles), keyed by model name.
pub(crate) type NamedLlmCache =
    Arc<crate::infrastructure::model_cache::ModelCache<String, Arc<dyn LLMPort>>>;

/// Single-slot chat model cache.
pub(crate) type ChatLlmCache =
    Arc<crate::infrastructure::model_cache::ModelCache<(), Arc<dyn LLMPort>>>;

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
    pub(crate) embedding_runtime: Arc<crate::infrastructure::embedding_runtime::EmbeddingRuntime>,

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
        let embedding_runtime =
            Arc::new(crate::infrastructure::embedding_runtime::EmbeddingRuntime::new());
        let llm_cache: ChatLlmCache =
            Arc::new(crate::infrastructure::model_cache::ModelCache::new());
        let router_llm_cache = Arc::new(crate::infrastructure::model_cache::ModelCache::new());
        let utility_llm_cache = Arc::new(crate::infrastructure::model_cache::ModelCache::new());
        tracing::info!("Building Container with modular architecture (Hollow Container Pattern)");

        // CoreModule builds shared singletons (security, runtime, credentials)
        // Stack frame for this construction is freed before Layer 2 begins
        let core = Arc::new(
            super::modules::CoreModule::new(db_pool.clone(), db_conn.clone(), data_dir.clone())
                .await?,
        );

        tracing::debug!("CoreModule initialized");

        // Each module builds its own dependencies. Stack frames are independent.

        let system =
            Arc::new(super::modules::SystemModule::new(db_pool.clone(), core.clone()).await?);
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

        let file_ops =
            Arc::new(super::modules::FileOpsModule::new(db_pool.clone(), core.clone()).await?);
        tracing::debug!("FileOpsModule initialized");

        let ai = Arc::new(
            super::modules::AIModule::new(
                db_pool.clone(),
                core.clone(),
                llm_cache.clone(),
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
                embedding_runtime.clone(),
                search.vector_search().clone(),
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
            web_service.clone(),
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
                llm_cache: llm_cache.clone(),
                router_llm_cache: router_llm_cache.clone(),
                utility_llm_cache: utility_llm_cache.clone(),
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
            web_service,
            router_llm_cache,
            utility_llm_cache,
            llm_cache,
            embedding_runtime,
            vault_writer,
            vault_write_suppression,
            settings_side_effects,
            app_handle: None,
        })
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

        let count = roots.len();
        self.file_access_config().set_allowed_roots(roots)?;
        tracing::info!(count, "recomputed file-access allowed roots");
        Ok(())
    }
}
