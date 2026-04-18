use crate::application::ports::{DocumentRepository, MentionRepositoryPort, RepositoryPort};
use crate::features::indexing::use_cases::IndexFileUseCase;
use crate::domain::entities::Document;
use crate::domain::events::model_download_events::ModelDownloadEvent;
use crate::infrastructure::event_bus::EventBus;
use crate::features::download::events::infra_events::DownloadEventBridge;
use crate::infrastructure::indexing;
use crate::infrastructure::persistence::repositories::summary_repository::SummaryRepository;
use crate::infrastructure::persistence::repositories::DocumentRepository as DddDocumentRepository;
use crate::infrastructure::sagas::conversation_summary_saga::ConversationSummarySaga;
use crate::features::download::manager::DownloadManager;
use crate::features::download::saga::DownloadSaga;
#[cfg(test)]
use crate::infrastructure::services::mocks::MockSearchService;
use crate::infrastructure::services::traits::{
    BM25SearchTrait, EmbeddingServiceTrait, FileStorageServiceTrait, HybridSearchTrait,
    IndexStorageTrait, ModelManagerTrait, SearchEnrichmentServiceTrait, SearchServiceTrait,
};
use crate::features::tags::{TagRepositoryTrait, TagServiceTrait};
use crate::interfaces::commands;
// ChunkRepositoryTrait removed - migrated to DDD ports
use crate::infrastructure::search::bm25::BM25Search;
use crate::infrastructure::search::hybrid::HybridSearchService;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::Manager;

/// Loads application configuration from the specified path.
/// Creates a default configuration if the file doesn't exist or is invalid.
///
/// # Arguments
/// * `config_path` - Path to the config.json file
///
/// # Returns
/// * `AppConfig` - The loaded or default configuration
fn load_config(config_path: PathBuf) -> commands::config::AppConfig {
    if config_path.exists() {
        match std::fs::read_to_string(&config_path) {
            Ok(contents) => {
                serde_json::from_str::<commands::config::AppConfig>(&contents).unwrap_or_default()
            }
            Err(_) => commands::config::AppConfig::default(),
        }
    } else {
        commands::config::AppConfig::default()
    }
}

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

/// Initializes the embedding service layer with the model directory.
///
/// # Arguments
/// * `model_dir` - Directory containing the embedding model files
///
/// # Returns
/// * `Option<Arc<EmbeddingService>>` - Embedding service if successful, None otherwise
async fn initialize_embedding_layer(
    model_dir: &Path,
) -> Option<Arc<crate::features::embedding::service::EmbeddingService>> {
    tracing::info!("Loading embedding models...");
    super::setup_embedding_service(model_dir).await
}

/// Initializes the tokenizer layer with the model directory.
///
/// # Arguments
/// * `model_dir` - Directory containing the tokenizer files
///
/// # Returns
/// * `Option<Arc<Tokenizer>>` - Tokenizer instance if available
fn initialize_tokenizer_layer(model_dir: &Path) -> Option<std::sync::Arc<tokenizers::Tokenizer>> {
    tracing::info!("Initializing tokenizer...");
    match super::setup_tokenizer(model_dir) {
        Some(tokenizer) => {
            tracing::info!("Tokenizer initialized successfully");
            Some(tokenizer)
        }
        None => {
            tracing::info!("Tokenizer not available - text processing will be limited");
            None
        }
    }
}

/// Initializes the indexing service layer with all required dependencies.
///
/// # Arguments
/// * `pool` - SQLite connection pool
/// * `app_dir` - Application data directory
/// * `embedder` - Embedding service instance (optional)
/// * `tokenizer` - Tokenizer instance (optional)
///
/// # Returns
/// * `Option<IndexingService>` - Initialized indexing service if both embedder and tokenizer are available
fn initialize_indexing_layer(
    pool: sqlx::SqlitePool,
    app_dir: PathBuf,
    embedder: Option<Arc<crate::features::embedding::service::EmbeddingService>>,
    tokenizer: Option<Arc<tokenizers::Tokenizer>>,
) -> Option<indexing::IndexingService> {
    match (embedder, tokenizer) {
        (Some(embedder), Some(tokenizer)) => {
            tracing::info!("Starting indexing service...");
            let service = indexing::IndexingService::new(pool, app_dir, embedder, tokenizer, 1000);
            tracing::info!("Indexing service started successfully");
            Some(service)
        }
        (None, _) => {
            tracing::warn!("Indexing service not initialized - embedding service unavailable");
            None
        }
        (_, None) => {
            tracing::warn!("Indexing service not initialized - tokenizer unavailable");
            None
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
    security_context: Arc<crate::security::SecurityContext>,
    model_dir: PathBuf,
    data_dir: PathBuf,
    config: commands::config::AppConfig,
) -> Result<crate::interfaces::di::Container, String> {
    use crate::interfaces::di::Container;

    tracing::info!("Creating DI Container (pure DDD architecture)");

    // Check if AI models are available
    let embedding_model_path = model_dir.join("model.onnx");
    let embedding_model_path_opt = if embedding_model_path.exists() {
        tracing::info!("✅ AI models found at {:?}", embedding_model_path);
        Some(embedding_model_path.to_string_lossy().to_string())
    } else {
        tracing::info!("ℹ️ AI models not found at {:?}", embedding_model_path);
        tracing::info!("AI features will use degraded mocks until models are downloaded");
        None
    };

    // Create container (always succeeds, AI optional)
    Container::new(
        pool,
        db_conn,
        embedding_model_path_opt,
        &config.ollama_endpoint,
        &config.ollama_model,
        data_dir,
    )
    .await
    .map_err(|e| format!("Failed to create container: {}", e))
}

/// Initializes the configuration service layer.
///
/// # Arguments
/// * `config_path` - Path to the config.json file
/// * `fallback_path` - Fallback path if primary fails
///
/// # Returns
/// * `Result<ConfigService, String>` - Config service or error with troubleshooting info
fn initialize_config_service(
    config_path: PathBuf,
    fallback_path: PathBuf,
) -> Result<commands::config::ConfigService, String> {
    match commands::config::ConfigService::new(config_path.clone()) {
        Ok(service) => Ok(service),
        Err(e) => {
            tracing::warn!(
                "Failed to initialize config service: {}. Creating new config at fallback path.",
                e
            );
            match commands::config::ConfigService::new(fallback_path) {
                Ok(service) => Ok(service),
                Err(e2) => Err(format!(
                    "Failed to initialize config service.\n\n\
                         Possible causes:\n\
                         - Cannot write to config directory\n\
                         - Disk full or read-only\n\
                         - Permission denied\n\n\
                         Error 1: {}\n\
                         Error 2: {}",
                    e, e2
                )),
            }
        }
    }
}

/// Orchestrates the sequential initialization of all application layers.
///
/// This function is the main entry point for application initialization.
/// It coordinates the setup of directories, database, embedding models,
/// tokenizer, indexing service, security context, and configuration.
///
/// # Architecture
/// The initialization follows a layered approach where each layer is
/// independently testable and has a single responsibility:
///
/// 1. **Directory Setup** - Ensures app and model directories exist
/// 2. **Config Loading** - Loads or creates default configuration
/// 3. **Database Layer** - Initializes SQLite database
/// 4. **Embedding Layer** - Loads ONNX embedding models
/// 5. **Tokenizer Layer** - Initializes text tokenizer
/// 6. **Indexing Layer** - Creates document indexing service
/// 7. **Security Layer** - Initializes security context
/// 8. **Config Service** - Sets up configuration persistence
///
/// # Async Memory Model
///
/// Async functions store their state on the HEAP (in Future objects), not the stack.
/// Deep async call chains don't cause stack overflow - that's the whole point of async.
/// Tauri's async runtime handles this correctly without needing custom runtimes.
#[tracing::instrument(skip(app))]
pub fn initialize_app(app: &mut tauri::App) -> Result<(), Box<dyn Error>> {
    let app_handle = app.handle().clone();

    // Use Tauri's built-in async runtime for initialization.
    // Async functions store state on heap via Futures, not stack.
    // Box::pin ensures the Future is heap-allocated.
    let result = tauri::async_runtime::block_on(async move {
        let future = Box::pin(initialize_app_async(app_handle.clone()));
        future.await
    });

    result.map_err(|e| Box::new(std::io::Error::other(e)) as Box<dyn Error>)?;

    tracing::info!("✅ Application initialization complete");
    Ok(())
}

/// Async initialization logic
///
/// This async function stores state on the heap (in Future objects), not the stack.
/// Components go directly to heap via Tauri State.
#[tracing::instrument(skip(app_handle))]
async fn initialize_app_async(app_handle: tauri::AppHandle) -> Result<(), String> {
    // Setup directories
    let app_dir = match super::setup_app_directories(&app_handle) {
        Ok(dir) => dir,
        Err(e) => {
            super::show_error_dialog(&app_handle, "Application Setup Failed", &e);
            return Err(e);
        }
    };

    crate::infrastructure::crash::set_crashes_directory(app_dir.clone());

    let model_dir = match super::setup_model_directory(&app_handle) {
        Ok(dir) => dir,
        Err(e) => {
            super::show_error_dialog(&app_handle, "Application Setup Failed", &e);
            return Err(e);
        }
    };

    let db_path = app_dir.join("vault.db");
    let config_path = app_dir.join("config.json");
    let config_file_path = app_dir.join("config.json");

    // Load configuration (synchronous)
    let config = load_config(config_path.clone());

    let model_dir_for_init = model_dir.clone();

    // Initialize all async layers with timeout
    let container = match tokio::time::timeout(Duration::from_secs(30), async move {
        // Sequential initialization with clear error propagation
        let conn = initialize_database_layer(db_path).await?;
        let embedder = initialize_embedding_layer(&model_dir_for_init).await; // Returns Option
        let tokenizer = initialize_tokenizer_layer(&model_dir_for_init); // Returns Option

        let indexing_service = initialize_indexing_layer(
            conn.pool().clone(),
            app_dir.clone(),
            embedder.clone(),
            tokenizer.clone(),
        );
        let security_context = initialize_security_layer();

        // === DUAL CONTAINER STRATEGY ===
        // During DDD migration, we run BOTH containers:
        // 1. Legacy ServiceContainer - for existing commands
        // 2. DDD Container - for new DDD-aligned commands
        // This allows gradual migration without breaking existing functionality.

        tracing::info!("🔧 Initializing DI Container (pure DDD architecture)");

        // Create unified DI Container
        let container = create_container(
            conn.pool().clone(),
            Arc::new(conn),
            security_context,
            model_dir_for_init.clone(),
            app_dir.clone(),
            config,
        )
        .await?;

        tracing::info!("✓ DI Container initialized");

        let batch_repo = container.batch_job_repository();
        tokio::spawn(async move {
            match batch_repo.list_batch_jobs(Some(500), Some(0)).await {
                Ok(jobs) => {
                    for job in jobs.into_iter().filter(|job| job.status == "running") {
                        if let Err(e) = batch_repo
                            .update_job_status(
                                &job.id,
                                "failed",
                                None,
                                Some(Utc::now().to_rfc3339()),
                            )
                            .await
                        {
                            tracing::warn!(
                                job_id = %job.id,
                                error = %e,
                                "Failed to mark stale batch job as failed"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "Failed to list batch jobs for lifecycle cleanup"
                    );
                }
            }
        });

        // Log AI model status
        if embedder.is_none() {
            tracing::info!("ℹ️ Application started without AI models");
            tracing::info!("Download models from Settings → Models to enable AI features");
        } else {
            tracing::info!("✅ Application started with full AI capabilities");
        }

        Ok::<crate::interfaces::di::Container, String>(container)
    })
    .await
    {
        Ok(Ok(container)) => container,
        Ok(Err(e)) => {
            super::show_error_dialog(&app_handle, "Application Initialization Failed", &e);
            return Err(e);
        }
        Err(_) => {
            let e = "Initialization timed out after 30 seconds".to_string();
            super::show_error_dialog(&app_handle, "Application Initialization Failed", &e);
            return Err(e);
        }
    };

    if let Err(e) = container
        .system
        .startup_auto_backup_use_case()
        .execute()
        .await
    {
        tracing::warn!(error = %e, "Failed to start auto-backup scheduler");
    }

    // === MANAGE CONTAINER IN TAURI STATE ===
    // Pure DDD Container - all commands use this

    // Initialize download management
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
    let download_manager = container
        .download_manager()
        .map_err(|e| format!("Failed to get shared download manager: {}", e))?;

    // Background tasks use tokio::spawn - async state is on heap, not stack

    // Reconcile stale downloads from previous app session
    tracing::info!("Reconciling stale downloads from previous session...");
    let reconcile_repo = download_repository.clone();
    let reconcile_startup_ts = Utc::now();
    let reconcile_grace = chrono::Duration::seconds(30);
    let model_repo_for_cleanup = Arc::new(
        crate::features::download::downloaded_model_repository::DownloadedModelRepository::new(
            container.db_pool().clone()
        )
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

    tracing::info!("Initializing conversation summary saga...");
    let summary_repo = Arc::new(SummaryRepository::new(container.db_pool().clone()));
    let conversation_summary_saga = Arc::new(ConversationSummarySaga::new(
        container.conversation_event_bus(),
        summary_repo,
    ));
    let summary_saga_listener = Arc::clone(&conversation_summary_saga);
    tokio::spawn(async move {
        tracing::info!("ConversationSummarySaga event listener started");
        summary_saga_listener.start().await;
    });
    tracing::info!("Conversation summary saga initialized");

    // === Wire DownloadSaga into application initialization ===
    // DownloadSaga exists but was never subscribed to events, so completions were missed.

    tracing::info!("Initializing download event system...");

    // Step 1: Create EventBus for domain events
    let event_bus = Arc::new(EventBus::<
        crate::domain::events::model_download_events::ModelDownloadEvent,
    >::new());
    tracing::info!("EventBus created");

    // Step 2: Create repositories needed by DownloadSaga
    let downloaded_model_repository = Arc::new(
        crate::features::download::downloaded_model_repository::DownloadedModelRepository::new(
            container.db_pool().clone()
        )
    );

    let model_file_repository = Arc::new(
        crate::infrastructure::persistence::repositories::model_file::SqliteModelFileRepository::new(
            container.db_pool().clone()
        )
    ) as Arc<dyn crate::domain::repositories::unit_of_work::ModelFileRepositoryPort>;

    let uow_factory = Arc::new(
        crate::infrastructure::persistence::repositories::unit_of_work::SqliteUnitOfWorkFactory::new(
            container.db_pool().clone()
        )
    ) as Arc<dyn crate::domain::repositories::UnitOfWorkFactory>;

    // Step 3: Instantiate DownloadSaga with all dependencies
    let download_saga = Arc::new(DownloadSaga::new(
        Arc::clone(&event_bus),
        Arc::clone(&model_file_repository),
        Arc::clone(&uow_factory),
        Arc::clone(&downloaded_model_repository),
    ));
    tracing::info!("DownloadSaga initialized");

    let saga_for_listener = Arc::clone(&download_saga);
    tokio::spawn(async move {
        tracing::info!("DownloadSaga event listener started");
        saga_for_listener.start().await;
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
    tokio::spawn(async move {
        event_bridge.start().await;
    });
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

    // Initialize config service
    let config_service = match initialize_config_service(config_path, config_file_path) {
        Ok(service) => service,
        Err(e) => {
            super::show_error_dialog(&app_handle, "Config Service Failed", &e);
            return Err(e);
        }
    };
    app_handle.manage(config_service);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_config_default_when_missing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("nonexistent.json");

        let config = load_config(config_path);

        // Should return default config without panicking
        assert_eq!(config.ollama_endpoint, "http://localhost:11434");
    }

    #[test]
    fn test_load_config_default_when_invalid() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("invalid.json");
        std::fs::write(&config_path, "invalid json content").unwrap();

        let config = load_config(config_path);

        // Should return default config when JSON is invalid
        assert_eq!(config.ollama_endpoint, "http://localhost:11434");
    }

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
