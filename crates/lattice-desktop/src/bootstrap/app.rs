use crate::features::download::events::infra_events::DownloadEventBridge;
use crate::features::download::saga::DownloadSaga;
use crate::infrastructure::events::event_bus::EventBus;
use crate::infrastructure::setup::{setup_app_directories, setup_database, setup_model_directory};
use lattice_core::application::ports::SettingsRepositoryPort;
use lattice_core::shared::error::AppError;
use lattice_core::shared::persistence::Database;
use lattice_core::shared::runtime::supervised_task::supervise_cancellable;
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
    fn setup(error: AppError) -> Self {
        Self {
            title: "Application Setup Failed",
            message: error.to_string(),
        }
    }

    fn initialization(error: AppError) -> Self {
        Self {
            title: "Application Initialization Failed",
            message: error.to_string(),
        }
    }
}

/// Loads application configuration from the specified path.
/// Creates a default configuration if the file doesn't exist or is invalid.
async fn initialize_database_layer(
    db_path: PathBuf,
) -> Result<crate::infrastructure::persistence::database::DatabaseConnection, AppError> {
    tracing::info!("Initializing database...");
    match setup_database(db_path).await {
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
fn initialize_security_layer() -> Arc<crate::security::SecurityContext> {
    tracing::info!("Initializing security context...");
    let context = Arc::new(crate::security::SecurityContext::new());
    tracing::info!("Security context initialized successfully");
    context
}

/// Creates the unified DI Container.
///
/// This container supports optional AI models - when models aren't installed,
/// AI-dependent features return helpful error messages guiding users to download models.
async fn create_container(
    database: Database,
    _security_context: Arc<crate::security::SecurityContext>,
    data_dir: PathBuf,
    ollama_endpoint: String,
    ollama_model: String,
    app_handle: tauri::AppHandle,
    background: Arc<lattice_core::shared::runtime::background::BackgroundTasks>,
) -> Result<crate::interfaces::di::Container, AppError> {
    use crate::interfaces::di::Container;

    tracing::info!("Creating DI Container (pure DDD architecture)");

    // Create container (always succeeds, AI optional). Builder-style
    // `with_app_handle` attaches the Tauri AppHandle so the sidecar
    // dispatch path can spawn `llama-server` via tauri-plugin-shell.
    Container::new(
        database,
        Some(app_handle.clone()),
        &ollama_endpoint,
        &ollama_model,
        data_dir,
        background,
    )
    .await
    .map(|c| c.with_app_handle(app_handle))
    .map_err(|e| e.with_context("Failed to create container"))
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
    let background =
        Arc::new(lattice_core::shared::runtime::background::BackgroundTasks::default());
    let shutdown_token = background.token();
    app_handle.manage(Arc::clone(&background));
    app_handle.manage(shutdown_token.clone());
    let app_dir = setup_app_directories(&app_handle).map_err(StartupFailure::setup)?;

    crate::infrastructure::crash::set_crashes_directory(app_dir.clone());
    let settings_dir_for_summaries = app_dir.clone();

    let model_dir = setup_model_directory(&app_handle).map_err(StartupFailure::setup)?;

    let db_path = app_dir.join("lattice.db");

    let app_handle_for_container = app_handle.clone();
    let background_for_container = Arc::clone(&background);

    let container = async move {
        // Sequential initialization with clear error propagation
        let conn = initialize_database_layer(db_path).await?;
        let security_context = initialize_security_layer();

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
                .map_err(|e| e.with_context("Failed to read settings during startup"))?;
        let bootstrap_settings = bootstrap_settings_repo
            .get_all()
            .await
            .map_err(|e| e.with_context("Failed to load settings during startup"))?;
        let ollama_endpoint = bootstrap_settings.llm.ollama_url.clone();
        let ollama_model = bootstrap_settings.llm.model.clone();

        let container = create_container(
            conn.database().clone(),
            security_context,
            app_dir.clone(),
            ollama_endpoint,
            ollama_model,
            app_handle_for_container,
            background_for_container,
        )
        .await?;

        tracing::info!("✓ DI Container initialized");

        // File-access policy starts at "app data directory only"; widen it to
        // the user's vault and indexed folders now that settings are readable.
        // Without this, indexed documents can't be opened at all.
        if let Err(e) = container.refresh_allowed_roots().await {
            tracing::warn!(error = %e, "failed to compute file-access allowed roots");
        }

        Ok::<crate::interfaces::di::Container, AppError>(container)
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
            container.database().clone(),
        ),
    );
    // IMPORTANT: Use the same DownloadManager instance the container/use-cases use.
    // This keeps model downloads and download drawer events in sync.
    let download_manager = container.download_manager().map_err(|e| {
        StartupFailure::initialization(e.with_context("Failed to get shared download manager"))
    })?;

    // Several cleanups below delete files that no row of this database names:
    // model directories, imported files, frozen pages and chat pictures. A
    // leaked file costs disk and a deleted one cannot be recovered, so only the
    // database that owns the files may. A fresh one beside them, after a
    // corrupt database was moved aside or in a second copy of the app, would
    // take every one. Decided once, here: the gate stays shut otherwise, and
    // each cleanup reads it.
    let sweep_gate = Arc::clone(container.sweep_gate());
    match crate::features::indexing::library_owner::LibraryFiles::current() {
        Ok(files) => {
            super::startup_reconciliation::open_gate_if_owner(
                container.database(),
                &files,
                &sweep_gate,
            )
            .await;
        }
        Err(error) => {
            tracing::warn!(%error, "Not deleting files that no row of this database names: could not find the library");
        }
    }

    // Background tasks use tokio::spawn - async state is on heap, not stack

    // Reconcile stale downloads from previous app session
    tracing::info!("Reconciling stale downloads from previous session...");
    let reconcile_repo = download_repository.clone();
    let reconcile_startup_ts = Utc::now();
    let model_repo_for_cleanup = Arc::new(
        crate::infrastructure::persistence::repositories::downloaded_models::DownloadedModelRepository::new(
            container.database().clone(),
        ),
    );

    let model_dir_for_cleanup = model_dir.clone();
    let sweep_gate_for_cleanup = Arc::clone(&sweep_gate);
    background.spawn(async move {
        use super::startup_reconciliation::{
            reconcile_orphaned_files, reconcile_orphaned_sessions, reconcile_stale_downloads,
        };

        match reconcile_stale_downloads(reconcile_repo.clone(), reconcile_startup_ts).await {
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

        match reconcile_orphaned_files(
            model_repo_for_cleanup,
            &model_dir_for_cleanup,
            &sweep_gate_for_cleanup,
        )
        .await
        {
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

    // A question the last run quit under would show as in flight for good;
    // fail it so it can be retried. The question of a research turn stays: its
    // job is delivered again and answers it.
    let turns_database = container.database().clone();
    let turns_jobs = Arc::clone(container.jobs());
    background.spawn(async move {
        use super::startup_reconciliation::reconcile_stuck_turns;
        match reconcile_stuck_turns(&turns_database, turns_jobs.store()).await {
            Ok(count) if count > 0 => {
                tracing::info!(count, "Marked interrupted conversation turns as failed");
            }
            Ok(_) => {}
            Err(error) => {
                tracing::error!(%error, "Failed to reconcile interrupted conversation turns");
            }
        }
    });

    // A grounding check the last run quit under would show "Checking…" for
    // good; mark it not checked before the first conversation loads.
    let verification_database = container.database().clone();
    background.spawn(async move {
        use super::startup_reconciliation::reconcile_interrupted_verifications;
        match reconcile_interrupted_verifications(&verification_database).await {
            Ok(count) if count > 0 => {
                tracing::info!(count, "Marked interrupted grounding checks as not checked");
            }
            Ok(_) => {}
            Err(error) => {
                tracing::error!(%error, "Failed to reconcile interrupted grounding checks");
            }
        }
    });

    // Collect files nothing references any more: an import that crashed
    // before it committed, a snapshot or picture whose row is gone. Without
    // this they stay on disk forever and get packed into every backup.
    let jobs = Arc::clone(container.jobs());
    super::startup_reconciliation::LibrarySweeps {
        library_gc: container.library_gc(),
        snapshots: container.web_page_snapshots(),
        images: container.chat_images_service(),
    }
    .start(&sweep_gate, &background, async move {
        crate::features::conversation::chat::resumable_research_requests(jobs.store()).await
    });

    // Collect attachments whose conversation was deleted while they could not
    // be. They are owned by a chat that no longer exists, so they are hidden
    // from the library and reachable from no conversation's retrieval; the
    // conversation's delete recorded them, and nothing but this can free them.
    // A conversation row gone without that record orphans nothing.
    let orphan_scope = container.document_scope();
    let orphan_delete = container.delete_document_use_case();
    let orphan_intents =
        crate::infrastructure::storage::FileIntents::new(container.database().clone());
    background.spawn(async move {
        use super::startup_reconciliation::reconcile_orphaned_attachments;
        let delete = |document_id: String| {
            let orphan_delete = std::sync::Arc::clone(&orphan_delete);
            async move { orphan_delete.execute(document_id).await.map(|_| ()) }
        };
        match reconcile_orphaned_attachments(orphan_scope.as_ref(), &orphan_intents, delete).await {
            Ok((0, _)) => {}
            Ok((found, deleted)) => tracing::info!(
                found,
                deleted,
                "Swept attachments left by deleted conversations"
            ),
            Err(error) => {
                tracing::error!(%error, "Failed to sweep attachments of deleted conversations")
            }
        }
    });

    // App-wide shutdown signal. Cloned into every supervised task so
    // app shutdown (Tauri window close, OS signal, test teardown) can
    // preempt long-running event loops that would otherwise wait
    // passively for the bus to close. Stored in Tauri state so command
    // handlers / shutdown hooks can fire it.

    // DownloadSaga exists but was never subscribed to events, so completions were missed.

    tracing::info!("Initializing download event system...");

    let event_bus = Arc::new(EventBus::<
        crate::features::download::events::model_download_events::ModelDownloadEvent,
    >::new());
    tracing::info!("EventBus created");

    let downloaded_model_repository = Arc::new(
        crate::infrastructure::persistence::repositories::downloaded_models::DownloadedModelRepository::new(
            container.database().clone(),
        ),
    );

    let download_saga = Arc::new(
        DownloadSaga::new(
            Arc::clone(&event_bus),
            container.database().clone(),
            Arc::clone(&downloaded_model_repository),
        )
        .with_settings(Arc::clone(container.settings_repository())),
    );
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
    let bridge_handle = background.spawn(async move {
        event_bridge.start(bridge_cancel).await;
    });
    app_handle.manage(super::background_workers::BackgroundWorkers::new(
        std::iter::once(saga_handle).chain(bridge_handle).collect(),
    ));
    tracing::info!("Download event bridge started with EventBus integration");

    // Queue promotion must not run inside the event consumer: a full bounded
    // event channel would otherwise deadlock the consumer on its own sends.
    let queue_manager = download_manager.clone();
    let queue_cancel = shutdown_token.clone();
    background.spawn(async move {
        loop {
            tokio::select! {
                biased;
                _ = queue_cancel.cancelled() => break,
                _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {}
            }
            if let Err(error) = queue_manager.process_pending_queue().await {
                tracing::warn!(%error, "Download queue promotion failed");
            }
        }
    });

    let download_state =
        crate::features::download::commands::DownloadCommandState::new(download_manager);
    tracing::info!("Download management initialized");

    // Resume any queued sessions left in pending state from earlier attempts.
    if let Err(e) = download_state.manager.process_pending_queue().await {
        tracing::warn!("Failed to process pending download queue on startup: {}", e);
    }

    let embedding_state = crate::features::embedding::commands::EmbeddingState::new(
        container.embedding_runtime.clone(),
    );
    tracing::info!("📦 Managing DI Container in Tauri state");
    app_handle.manage(container);
    tracing::info!("✅ DI Container available for all commands");

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
    use crate::features::settings::repository::SettingsRepository;
    use lattice_core::application::ports::SettingsRepositoryPort;

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
