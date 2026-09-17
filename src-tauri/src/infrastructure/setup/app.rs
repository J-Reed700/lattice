use crate::application::ports::SettingsRepositoryPort;
use crate::features::download::events::infra_events::DownloadEventBridge;
use crate::features::download::saga::DownloadSaga;
use crate::infrastructure::event_bus::EventBus;
use crate::shared::utils::supervised_task::supervise_cancellable;
use tokio_util::sync::CancellationToken;
// ChunkRepositoryTrait removed - migrated to DDD ports
use chrono::Utc;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;

/// A startup failure, with the dialog title the user sees before the process exits.
struct StartupFailure {
    title: &'static str,
    message: String,
}

impl StartupFailure {
    fn setup(message: String) -> Self {
        Self {
            title: "Application Setup Failed",
            message,
        }
    }

    fn initialization(message: String) -> Self {
        Self {
            title: "Application Initialization Failed",
            message,
        }
    }
}

/// Loads application configuration from the specified path.
/// Creates a default configuration if the file doesn't exist or is invalid.
///
/// # Arguments
/// Initializes the database layer with the specified database path.
///
/// # Arguments
/// * `db_path` - Path to the SQLite database file
///
/// # Returns
/// * `Result<DatabaseConnection, String>` - Database connection or error
async fn initialize_database_layer(
    db_path: PathBuf,
) -> Result<crate::infrastructure::persistence::database::DatabaseConnection, String> {
    tracing::info!("Initializing database...");
    match super::setup_database(db_path).await {
        Ok(conn) => {
            tracing::info!("Database initialized successfully");
            Ok(conn)
        }
        Err(e) => {
            tracing::error!(error = %e, "Database initialization failed");
            Err(e)
        }
    }
}

/// Initializes the security context layer.
///
/// # Returns
/// * `Arc<SecurityContext>` - Initialized security context (never fails)
fn initialize_security_layer() -> Arc<crate::security::SecurityContext> {
    tracing::info!("Initializing security context...");
    let context = Arc::new(crate::security::SecurityContext::new());
    tracing::info!("Security context initialized successfully");
    context
}

/// Creates the DDD Container (new architecture).
///
/// This is the NEW DDD-aligned container that will replace the legacy one.
/// It follows proper layering: Domain → Application → Infrastructure → Interfaces.
///
/// During migration:
///   - Legacy commands use legacy ServiceContainer
///
/// Creates the unified DI Container (pure DDD architecture).
///
/// This container supports optional AI models - when models aren't installed,
/// AI-dependent features return helpful error messages guiding users to download models.
///
/// # Arguments
/// * `pool` - SQLite connection pool
/// * `security_context` - Security context with rate limiters
/// * `model_dir` - Directory containing ML models (checks for model.onnx)
/// * `data_dir` - Application data directory
/// * `config` - Application configuration
///
/// # Returns
/// * `Result<Container, String>` - Container (always succeeds, AI features optional)
async fn create_container(
    pool: sqlx::SqlitePool,
    db_conn: Arc<crate::infrastructure::persistence::database::DatabaseConnection>,
    _security_context: Arc<crate::security::SecurityContext>,
    model_dir: PathBuf,
    data_dir: PathBuf,
    ollama_endpoint: String,
    ollama_model: String,
    app_handle: tauri::AppHandle,
) -> Result<crate::interfaces::di::Container, String> {
    use crate::interfaces::di::Container;

    tracing::info!("Creating DI Container (pure DDD architecture)");

    let embedding_model_path = model_dir.join("model.onnx");
    let embedding_model_path_opt = if embedding_model_path.exists() {
        tracing::info!("✅ AI models found at {:?}", embedding_model_path);
        Some(embedding_model_path.to_string_lossy().to_string())
    } else {
        tracing::info!("Optional ONNX model absent at {:?}", embedding_model_path);
        tracing::info!("Configured embedding and chat providers will be resolved independently");
        None
    };

    // Create container (always succeeds, AI optional). Builder-style
    // `with_app_handle` attaches the Tauri AppHandle so the sidecar
    // dispatch path can spawn `llama-server` via tauri-plugin-shell.
    Container::new(
        pool,
        db_conn,
        embedding_model_path_opt,
        &ollama_endpoint,
        &ollama_model,
        data_dir,
    )
    .await
    .map(|c| c.with_app_handle(app_handle))
    .map_err(|e| format!("Failed to create container: {}", e))
}

/// Orchestrates the sequential initialization of all application layers.
///
/// This function is the main entry point for application initialization.
/// It coordinates the setup of directories, database, security context,
/// and configuration.
///
/// # Architecture
/// The initialization follows a layered approach where each layer is
/// independently testable and has a single responsibility:
///
/// 1. **Directory Setup** - Ensures app and model directories exist
/// 2. **Config Loading** - Loads or creates default configuration
/// 3. **Database Layer** - Initializes SQLite database
/// 4. **Security Layer** - Initializes security context
/// 5. **Config Service** - Sets up configuration persistence
///
/// # Async Memory Model
///
/// Async functions store their state on the HEAP (in Future objects), not the stack.
/// Deep async call chains don't cause stack overflow - that's the whole point of async.
/// Tauri's async runtime handles this correctly without needing custom runtimes.
///
/// # Failure
///
/// Does not return on failure. Tauri turns a setup-hook `Err` into a panic
/// inside a macOS callback that cannot unwind, so the process would abort with
/// a crash report. Instead the failure is logged, then shown in a dialog that
/// blocks until dismissed, then the shared shutdown sequence runs and the
/// process exits with status 1. `AppHandle::exit(1)` is not an option: the
/// runtime reports any requested exit as status 0, and the run-event handler
/// reads renderer-shutdown state that only a successful startup installs.
#[tracing::instrument(skip(app))]
pub fn initialize_app(app: &mut tauri::App) {
    let app_handle = app.handle().clone();

    let result = tauri::async_runtime::block_on(async {
        let future = Box::pin(initialize_app_async(app_handle.clone()));
        future.await
    });

    if let Err(failure) = result {
        tracing::error!(error = %failure.message, "{}", failure.title);
        super::show_error_dialog(&app_handle, failure.title, &failure.message);
        super::graceful_shutdown(&app_handle);
        std::process::exit(1);
    }

    tracing::info!("✅ Application initialization complete");
}

/// Async initialization logic
///
/// This async function stores state on the heap (in Future objects), not the stack.
/// Components go directly to heap via Tauri State.
#[tracing::instrument(skip(app_handle))]
async fn initialize_app_async(app_handle: tauri::AppHandle) -> Result<(), StartupFailure> {
    let app_dir = super::setup_app_directories(&app_handle).map_err(StartupFailure::setup)?;

    crate::infrastructure::crash::set_crashes_directory(app_dir.clone());
    let settings_dir_for_summaries = app_dir.clone();

    let model_dir = super::setup_model_directory(&app_handle).map_err(StartupFailure::setup)?;

    let db_path = app_dir.join("lattice.db");

    let model_dir_for_init = model_dir.clone();

    let app_handle_for_container = app_handle.clone();

    let container = async move {
        // Sequential initialization with clear error propagation
        let conn = initialize_database_layer(db_path).await?;
        let security_context = initialize_security_layer();

        // During DDD migration, we run BOTH containers:
        // 1. Legacy ServiceContainer - for existing commands
        // 2. DDD Container - for new DDD-aligned commands
        // This allows gradual migration without breaking existing functionality.

        tracing::info!("🔧 Initializing DI Container (pure DDD architecture)");

        // Read Ollama endpoint + model from the unified Settings store. This
        // also runs the one-shot migration of any leftover legacy config.json
        // (see SettingsRepository::new) so by the time we read here the
        // user's prior choices have been ported into settings.json.
        // This bootstrap repo is an idempotent reader of the same JSON file
        // the DI container's SettingsRepository instance will own — both
        // resolve to the same on-disk state, no split-brain.
        let bootstrap_settings_repo =
            crate::features::settings::repository::SettingsRepository::new(app_dir.clone())
                .await
                .map_err(|e| format!("Failed to read settings during startup: {e}"))?;
        let bootstrap_settings = bootstrap_settings_repo
            .get_all()
            .await
            .map_err(|e| format!("Failed to load settings during startup: {e}"))?;
        let ollama_endpoint = bootstrap_settings.llm.ollama_url.clone();
        let ollama_model = bootstrap_settings.llm.model.clone();

        let container = create_container(
            conn.pool().clone(),
            Arc::new(conn),
            security_context,
            model_dir_for_init.clone(),
            app_dir.clone(),
            ollama_endpoint,
            ollama_model,
            app_handle_for_container,
        )
        .await?;

        tracing::info!("✓ DI Container initialized");

        // File-access policy starts at "app data directory only"; widen it to
        // the user's vault and indexed folders now that settings are readable.
        // Without this, indexed documents can't be opened at all.
        if let Err(e) = container.refresh_allowed_roots().await {
            tracing::warn!(error = %e, "failed to compute file-access allowed roots");
        }

        // Resume interrupted file imports from their durable item records. Load
        // the selected embedding model before allowing the workers to continue.
        let batch_repo = container.batch_job_repository();
        match batch_repo.list_batch_jobs(Some(500), Some(0)).await {
            Ok(jobs) => {
                let active: Vec<_> = jobs.into_iter()
                    .filter(|job| matches!(job.status.as_str(), "pending" | "running"))
                    .collect();
                let needs_embedding = active.iter().any(|job| job.job_type == "file_import");
                let embedding_ready = if needs_embedding {
                    match container.get_or_load_embedding().await {
                        Ok(_) => true,
                        Err(error) => {
                            tracing::warn!(%error, "Cannot resume file imports without embedding model");
                            false
                        }
                    }
                } else { false };
                for job in active {
                    if job.job_type == "file_import" && embedding_ready {
                        if let Err(error) = container.start_batch_file_import_use_case().resume_interrupted(job.id.clone()).await {
                            tracing::warn!(job_id = %job.id, %error, "Failed to resume file import");
                        }
                    } else if let Err(error) = batch_repo.update_job_status(
                        &job.id, "failed", None, Some(Utc::now().to_rfc3339())
                    ).await {
                        tracing::warn!(job_id = %job.id, %error, "Failed to mark interrupted batch job failed");
                    }
                }
            }
            Err(error) => tracing::warn!(%error, "Failed to recover batch jobs"),
        }

        Ok::<crate::interfaces::di::Container, String>(container)
    }
    .await
    .map_err(StartupFailure::initialization)?;

    if let Err(e) = container
        .system
        .startup_auto_backup_use_case()
        .execute()
        .await
    {
        tracing::warn!(error = %e, "Failed to start auto-backup scheduler");
    }

    // Pure DDD Container - all commands use this

    tracing::info!("Initializing download management...");
    let download_repository: Arc<
        dyn crate::features::download::download_repository::DownloadRepository,
    > = Arc::new(
        crate::features::download::download_repository::SqliteDownloadRepository::new(
            container.db_conn().clone(),
        ),
    );
    // IMPORTANT: Use the same DownloadManager instance the container/use-cases use.
    // This keeps model downloads and download drawer events in sync.
    let download_manager = container.download_manager().map_err(|e| {
        StartupFailure::initialization(format!("Failed to get shared download manager: {}", e))
    })?;

    // Background tasks use tokio::spawn - async state is on heap, not stack

    // Reconcile stale downloads from previous app session
    tracing::info!("Reconciling stale downloads from previous session...");
    let reconcile_repo = download_repository.clone();
    let reconcile_startup_ts = Utc::now();
    let reconcile_grace = chrono::Duration::seconds(30);
    let model_repo_for_cleanup = Arc::new(
        crate::features::download::downloaded_model_repository::DownloadedModelRepository::new(
            container.db_pool().clone(),
        ),
    );

    let model_dir_for_cleanup = model_dir.clone();
    tokio::spawn(async move {
        use crate::infrastructure::services::startup_reconciliation::{
            reconcile_orphaned_files, reconcile_orphaned_sessions, reconcile_stale_downloads,
        };

        match reconcile_stale_downloads(
            reconcile_repo.clone(),
            reconcile_startup_ts,
            reconcile_grace,
        )
        .await
        {
            Ok(count) if count > 0 => {
                tracing::info!("Reconciled {} stale download(s) on startup", count);
            }
            Ok(_) => {
                tracing::debug!("No stale downloads to reconcile");
            }
            Err(e) => {
                tracing::error!("Failed to reconcile stale downloads: {}", e);
            }
        }

        match reconcile_orphaned_sessions(reconcile_repo, model_repo_for_cleanup.clone()).await {
            Ok(count) if count > 0 => {
                tracing::info!("Cleaned up {} orphaned session(s) on startup", count);
            }
            Ok(_) => {
                tracing::debug!("No orphaned sessions to clean up");
            }
            Err(e) => {
                tracing::error!("Failed to clean orphaned sessions: {}", e);
            }
        }

        match reconcile_orphaned_files(model_repo_for_cleanup, &model_dir_for_cleanup).await {
            Ok(count) if count > 0 => {
                tracing::info!("Cleaned up {} orphaned model file(s) on startup", count);
            }
            Ok(_) => {
                tracing::debug!("No orphaned model files to clean up");
            }
            Err(e) => {
                tracing::error!("Failed to clean orphaned model files: {}", e);
            }
        }
    });

    // Collect library blobs nothing references any more: an import that
    // crashed before it committed, or a database that was reset out from under
    // the files. Without this they stay on disk forever and get packed into
    // every backup.
    let library_gc = container.library_gc();
    tokio::spawn(async move {
        match library_gc.sweep().await {
            Ok(report) => tracing::info!(
                removed = report.removed,
                retained = report.retained,
                bytes_freed = report.bytes_freed,
                "Swept the imported-file library"
            ),
            Err(error) => {
                tracing::error!(%error, "Failed to sweep the imported-file library");
            }
        }
    });

    // App-wide shutdown signal. Cloned into every supervised task so
    // app shutdown (Tauri window close, OS signal, test teardown) can
    // preempt long-running event loops that would otherwise wait
    // passively for the bus to close. Stored in Tauri state so command
    // handlers / shutdown hooks can fire it.
    let shutdown_token = CancellationToken::new();
    app_handle.manage(shutdown_token.clone());

    // DownloadSaga exists but was never subscribed to events, so completions were missed.

    tracing::info!("Initializing download event system...");

    let event_bus = Arc::new(EventBus::<
        crate::features::download::events::model_download_events::ModelDownloadEvent,
    >::new());
    tracing::info!("EventBus created");

    let downloaded_model_repository = Arc::new(
        crate::features::download::downloaded_model_repository::DownloadedModelRepository::new(
            container.db_pool().clone(),
        ),
    );

    let model_file_repository = Arc::new(
        crate::infrastructure::persistence::repositories::model_file::SqliteModelFileRepository::new(
            container.db_pool().clone()
        )
    ) as Arc<dyn crate::application::ports::unit_of_work::ModelFileRepositoryPort>;

    let uow_factory = Arc::new(
        crate::infrastructure::persistence::repositories::unit_of_work::SqliteUnitOfWorkFactory::new(
            container.db_pool().clone()
        )
    ) as Arc<dyn crate::application::ports::UnitOfWorkFactory>;

    let download_saga = Arc::new(DownloadSaga::new(
        Arc::clone(&event_bus),
        Arc::clone(&model_file_repository),
        Arc::clone(&uow_factory),
        Arc::clone(&downloaded_model_repository),
    ));
    tracing::info!("DownloadSaga initialized");

    let saga_for_listener = Arc::clone(&download_saga);
    let download_saga_cancel = shutdown_token.clone();
    let saga_handle = supervise_cancellable("download_saga", shutdown_token.clone(), move || {
        let saga = Arc::clone(&saga_for_listener);
        let cancel = download_saga_cancel.clone();
        async move {
            tracing::info!("DownloadSaga event listener started");
            saga.start(cancel).await;
        }
    });

    tracing::info!("Starting download event bridge...");
    let event_rx_arc = download_manager.subscribe_to_events();
    let event_bridge = DownloadEventBridge::new(
        app_handle.clone(),
        download_manager.clone(),
        download_repository,
        event_rx_arc,
        Some(downloaded_model_repository),
        Some(event_bus),
    );
    // NOT supervised. The bridge's `start(self)` consumes self and
    // takes the manager-event receiver via `.take()` on first call —
    // a restart would find the receiver gone and exit immediately.
    // The recv loops inside the bridge now handle Lagged/Closed
    // correctly (see P0 fix in this file's git log), so panics during
    // steady-state operation are unlikely. Refactoring the bridge to
    // be restartable is a separate task.
    let bridge_cancel = shutdown_token.clone();
    let bridge_handle = tokio::spawn(async move {
        event_bridge.start(bridge_cancel).await;
    });
    app_handle.manage(super::background_workers::BackgroundWorkers::new(vec![
        saga_handle,
        bridge_handle,
    ]));
    tracing::info!("Download event bridge started with EventBus integration");

    let download_state =
        crate::features::download::commands::DownloadCommandState::new(download_manager);
    tracing::info!("Download management initialized");

    // Resume any queued sessions left in pending state from earlier attempts.
    if let Err(e) = download_state.manager.process_pending_queue().await {
        tracing::warn!("Failed to process pending download queue on startup: {}", e);
    }

    tracing::info!("📦 Managing DI Container in Tauri state");
    app_handle.manage(container);
    tracing::info!("✅ DI Container available for all commands");

    let embedding_state = crate::features::embedding::commands::EmbeddingState::default();
    app_handle.manage(embedding_state);

    app_handle.manage(download_state);

    if let Some(container) = app_handle.try_state::<crate::interfaces::di::Container>() {
        container.prewarm_active_models(app_handle.clone());
    } else {
        tracing::warn!("prewarm: Container missing from Tauri state immediately after manage()");
    }

    let summaries_enabled = read_summary_tier_flag(&settings_dir_for_summaries).await;
    if let Err(error) =
        crate::features::summaries::di::register(app_handle.clone(), summaries_enabled).await
    {
        tracing::warn!(%error, "Could not enable the document summary tier");
    }

    Ok(())
}

/// Read the stored `summary_index_enabled` search setting, defaulting to off
/// when settings cannot be read so startup never depends on it.
async fn read_summary_tier_flag(data_dir: &std::path::Path) -> bool {
    use crate::application::ports::SettingsRepositoryPort;
    use crate::infrastructure::persistence::repositories::SettingsRepository;

    match SettingsRepository::new(data_dir.to_path_buf()).await {
        Ok(repository) => match repository.get_all().await {
            Ok(settings) => settings.search.summary_index_enabled,
            Err(e) => {
                tracing::warn!(error = %e, "Could not read search settings; summary tier stays off");
                false
            }
        },
        Err(e) => {
            tracing::warn!(error = %e, "Could not open the settings repository; summary tier stays off");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_initialize_database_layer() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let result = initialize_database_layer(db_path).await;

        // Database initialization should succeed
        assert!(result.is_ok());
    }

    #[test]
    fn test_initialize_security_layer() {
        let context = initialize_security_layer();

        // Security context should always initialize successfully
        assert!(Arc::strong_count(&context) >= 1);
    }
}
