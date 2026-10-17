//! LLM / model-management feature dependency injection.

use crate::features::llm::engine::system::BackendDevices;
use crate::features::llm::instances::{LearningShortfall, LoadedInstance, SidecarBudget};
use crate::features::llm::loading::ModelLoader;
use lattice_core::shared::runtime::background::BackgroundTasks;
use std::sync::Arc;

use sqlx::SqlitePool;

use crate::features::download::download_repository::SqliteDownloadRepository;
use crate::features::download::downloaded_model_repository::DownloadedModelRepository;
use crate::features::download::engine::HttpDownloadEngine;
use crate::features::download::manager::{DownloadManager, DownloadManagerService};
use crate::features::llm::use_cases::{
    CheckModelDownloadedUseCase, DeleteModelUseCase, DownloadModelUseCase,
    GetAvailableModelsUseCase, GetBestModelUseCase, GetModelPathUseCase,
    GetRecommendedModelsUseCase, GetSystemCapabilitiesUseCase, ListDownloadedModelsUseCase,
};
use crate::features::model_management::cache_adapter::ModelCacheAdapter;
use crate::features::model_management::huggingface_adapter::HuggingFaceAdapter;
use crate::infrastructure::adapters::fs::tokio_checksum::TokioChecksumAdapter;
use crate::infrastructure::adapters::fs::TokioFileSystemAdapter;
use crate::infrastructure::adapters::system_info::SystemInfoAdapter;
use crate::infrastructure::file_system::FileSystemAdapter;
use crate::infrastructure::persistence::database::DatabaseConnection;
use crate::infrastructure::persistence::repositories::unit_of_work::SqliteUnitOfWorkFactory;
use crate::interfaces::di::Container;
use lattice_core::application::ports::llm_port::{
    InvalidationCause, ModelRequirements, ModelRole, OptionalLlmLoader, PinnedModel,
};
use lattice_core::application::ports::UnitOfWorkFactory;
use lattice_core::application::ports::{
    CredentialsPort, FileSystemPort, LLMPort, ModelCatalogPort, ModelStoragePort, SystemInfoPort,
};
use lattice_core::domain::conversation::ChatModelChoice;
use lattice_core::domain::models::model_ref::ModelRef;
use lattice_core::domain::ports::file_access::{ChecksumService, FileSystemAccess};
use lattice_core::domain::repositories::downloaded_model_repository::DownloadedModelRepository as DownloadedModelRepositoryPort;
use lattice_core::shared::error::{AppError, Result};
use std::path::PathBuf;

mod roles;

use roles::{ModelRequest, ModelSelection, RoleModels};

#[derive(Clone)]
pub struct LlmDi {
    pub get_system_capabilities_use_case: Arc<GetSystemCapabilitiesUseCase>,
    pub get_available_models_use_case: Arc<GetAvailableModelsUseCase>,
    pub get_recommended_models_use_case: Arc<GetRecommendedModelsUseCase>,
    pub get_best_model_use_case: Arc<GetBestModelUseCase>,
    pub check_model_downloaded_use_case: Arc<CheckModelDownloadedUseCase>,
    pub get_model_path_use_case: Arc<GetModelPathUseCase>,
    pub download_model_use_case: Arc<DownloadModelUseCase>,
    pub delete_model_use_case: Arc<DeleteModelUseCase>,
    pub list_models_use_case: Arc<ListDownloadedModelsUseCase>,

    // Shared state / adapters
    pub downloaded_model_repo: Arc<DownloadedModelRepository>,
    pub model_catalog: Arc<dyn ModelCatalogPort>,
    pub model_catalog_cache: Arc<ModelCacheAdapter>,
    pub download_manager: Arc<dyn DownloadManager>,
}

pub async fn build(
    db_pool: SqlitePool,
    db_conn: Arc<DatabaseConnection>,
    data_dir: PathBuf,
    credentials: Arc<dyn CredentialsPort>,
    backend_devices: Arc<BackendDevices>,
    background: Arc<BackgroundTasks>,
) -> Result<LlmDi> {
    // Model catalog (HuggingFace + SQLite cache)
    let downloaded_model_repo = Arc::new(DownloadedModelRepository::new(db_pool.clone()));

    let huggingface = Arc::new(HuggingFaceAdapter::new()) as Arc<dyn ModelCatalogPort>;
    let model_catalog_cache = Arc::new(ModelCacheAdapter::new(db_pool.clone(), huggingface).await?);
    let model_catalog = model_catalog_cache.clone() as Arc<dyn ModelCatalogPort>;

    let system_info = Arc::new(SystemInfoAdapter::new(backend_devices)) as Arc<dyn SystemInfoPort>;

    // Download wiring
    let download_repository = Arc::new(SqliteDownloadRepository::new(db_conn));
    let download_engine = Arc::new(HttpDownloadEngine::new()?);
    let download_manager = Arc::new(DownloadManagerService::new(
        download_repository,
        download_engine,
        data_dir,
        background,
    )) as Arc<dyn DownloadManager>;

    let file_system = Arc::new(FileSystemAdapter::new()) as Arc<dyn FileSystemPort>;
    let checksum_service = Arc::new(TokioChecksumAdapter) as Arc<dyn ChecksumService>;
    let file_system_access = Arc::new(TokioFileSystemAdapter) as Arc<dyn FileSystemAccess>;
    let uow_factory = Arc::new(SqliteUnitOfWorkFactory::new(db_pool)) as Arc<dyn UnitOfWorkFactory>;

    let model_storage = downloaded_model_repo.clone() as Arc<dyn ModelStoragePort>;

    let get_system_capabilities_use_case = Arc::new(GetSystemCapabilitiesUseCase::new(system_info));
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
    let download_model_use_case = Arc::new(DownloadModelUseCase::new(
        model_catalog.clone(),
        model_storage.clone(),
        download_manager.clone(),
        file_system,
        credentials,
        checksum_service,
        file_system_access,
        uow_factory,
    ));
    let delete_model_use_case = Arc::new(DeleteModelUseCase::new(
        model_storage.clone(),
        downloaded_model_repo.clone() as Arc<dyn DownloadedModelRepositoryPort>,
    ));
    let list_models_use_case = Arc::new(ListDownloadedModelsUseCase::new(model_storage));

    Ok(LlmDi {
        get_system_capabilities_use_case,
        get_available_models_use_case,
        get_recommended_models_use_case,
        get_best_model_use_case,
        check_model_downloaded_use_case,
        get_model_path_use_case,
        download_model_use_case,
        delete_model_use_case,
        list_models_use_case,
        downloaded_model_repo,
        model_catalog,
        model_catalog_cache,
        download_manager,
    })
}

/// LLM's registrar surface on `Container`.
///
/// Which model each role runs on, the loaded instances and their
/// invalidation live here; the `Container` only owns the instances and the
/// `AppHandle` the sidecar factory needs.
impl Container {
    /// Returns the stored AppHandle, if any. Used by the LLM factory
    /// to populate `LLMConfig::Local.app_handle` when the sidecar
    /// feature is enabled.
    fn app_handle(&self) -> Option<tauri::AppHandle> {
        self.app_handle.clone()
    }

    fn model_loader(&self) -> ModelLoader {
        ModelLoader::new(
            self.ai.downloaded_model_repo().clone(),
            self.app_handle(),
            self.llm_runtime.clone(),
        )
    }

    fn role_models(&self) -> RoleModels {
        RoleModels {
            instances: Arc::clone(&self.llm_instances),
            loader: Arc::new(self.model_loader()),
            budget: Arc::new(SidecarBudget::new(
                self.app_handle(),
                Arc::clone(&self.llm_runtime.devices),
            )),
            settings: Arc::clone(self.system.settings_repo()),
        }
    }

    /// The model `request` asks for, pinned for its work.
    pub(crate) async fn llm_for(&self, request: &ModelRequest) -> Result<PinnedModel> {
        self.role_models().llm_for(request).await
    }

    /// The model a turn in `conversation_id` runs on, pinned for the turn:
    /// `request_override`, else the chat's choice, else the Settings default.
    pub async fn chat_llm_for(
        &self,
        conversation_id: &str,
        request_override: Option<&ModelRef>,
        requirements: ModelRequirements,
    ) -> Result<PinnedModel> {
        let (choice, title) = chat_model_choice(self.db_pool(), conversation_id).await?;
        self.llm_for(&ModelRequest {
            role: ModelRole::Chat,
            selection: ModelSelection::Chat {
                choice,
                request_override: request_override.cloned(),
            },
            requirements,
            work: title,
            conversation_id: Some(conversation_id.to_owned()),
        })
        .await
    }

    /// The Learning model, pinned for one job. Never a chat's choice or the
    /// chat default: unset, learning work refuses.
    pub async fn learning_llm(&self, requirements: ModelRequirements) -> Result<PinnedModel> {
        self.llm_for(&ModelRequest {
            role: ModelRole::Learning,
            selection: ModelSelection::Learning,
            requirements,
            work: String::new(),
            conversation_id: None,
        })
        .await
    }

    /// The Settings default chat model, for work that belongs to no chat.
    pub async fn default_chat_llm(&self) -> Result<Arc<dyn LLMPort>> {
        Ok(self.pinned_default_chat_llm().await?.llm)
    }

    /// The Settings default chat model with what it resolved to, for work
    /// that belongs to no chat and reports which model served it.
    pub async fn pinned_default_chat_llm(&self) -> Result<PinnedModel> {
        self.llm_for(&ModelRequest::default_chat()).await
    }

    /// The router model, loaded through the same instances as every role.
    pub async fn get_or_load_router_llm(&self) -> Result<Arc<dyn LLMPort>> {
        self.role_models().router_llm().await
    }

    /// Get the utility LLM if one is configured.
    pub async fn get_or_load_utility_llm(&self) -> Result<Option<Arc<dyn LLMPort>>> {
        (self.utility_llm_loader())().await
    }

    /// Bind only the dependencies needed by lazy utility-model consumers.
    pub(crate) fn utility_llm_loader(&self) -> OptionalLlmLoader {
        let models = self.role_models();
        Arc::new(move || {
            let models = models.clone();
            Box::pin(async move { models.utility_llm().await })
        })
    }

    /// Stop handing out the loaded models `cause` makes stale. Work that holds
    /// one keeps it until it ends; nothing in Settings changes.
    pub fn invalidate_llm_instances(&self, cause: InvalidationCause) {
        self.llm_instances.invalidate(&cause);
    }

    /// Every model loaded now, with who holds it.
    pub(crate) fn loaded_models(&self) -> Vec<LoadedInstance> {
        self.llm_instances.snapshot()
    }

    /// The last learning work refused because the Learning model's window
    /// was too small, while that model has not covered the window since.
    pub(crate) fn learning_shortfall(&self) -> Option<LearningShortfall> {
        self.llm_instances.learning_shortfall()
    }

    /// Load the Learning model ahead of the first learning job, reporting
    /// `model:warmup-status { role: "learning" }`. A failure is reported and
    /// changes no setting.
    pub async fn warm_up_learning_model(&self) -> Result<()> {
        let role = "learning";
        let emit = |phase, error, degraded| {
            if let Some(app) = self.app_handle() {
                emit_warmup(&app, role, phase, error, degraded);
            }
        };
        emit("started", None, None);
        match self.learning_llm(ModelRequirements::default()).await {
            Ok(pinned) => {
                emit("ready", None, pinned.degraded);
                Ok(())
            }
            Err(error) => {
                let message = match &error {
                    AppError::AiModelsNotInstalled(message)
                    | AppError::InvalidConfig(message)
                    | AppError::ModelLoadFailed(message)
                    | AppError::ServiceNotAvailable(message) => message.clone(),
                    _ => error.to_string(),
                };
                emit("failed", Some(message), None);
                Err(error)
            }
        }
    }

    /// Fire-and-forget per-role warmup at boot. Emits
    /// `model:warmup-status { role, phase, error? }` events with phases
    /// `started → ready | skipped | failed`. Errors never propagate.
    ///
    /// Also preflights the bundled llama-server builds, whether or not a
    /// local model is active, so every log says whether local models can
    /// run on this machine.
    pub fn prewarm_active_models(&self, app_handle: tauri::AppHandle) {
        use tauri::Manager;
        let background = self.core.background();
        crate::features::llm::engine::sidecar_manager::spawn_binary_preflight(
            &app_handle,
            Arc::clone(&self.llm_runtime.devices),
            background,
        );
        for role in ["chat", "utility", "embedding"] {
            let handle = app_handle.clone();
            let cancel = background.token();
            let _ = background.spawn(async move {
                if cancel.is_cancelled() {
                    return;
                }
                let container = match handle.try_state::<Container>() {
                    Some(c) => c,
                    None => {
                        tracing::warn!(role, "prewarm: Container not in Tauri state yet");
                        return;
                    }
                };
                emit_warmup(&handle, role, "started", None, None);
                let started = std::time::Instant::now();
                // Cancellation stops the prewarm waiter. Any sidecar whose
                // startup already crossed the process boundary is still
                // registered with SidecarRegistry, whose closed admission
                // kills late registrations during shutdown.
                let Some(outcome) = finish_or_cancel(&cancel, async {
                    match role {
                        "chat" => match container.llm_for(&ModelRequest::default_chat()).await {
                            Ok(pinned) => Outcome::Ready(pinned.degraded),
                            Err(e) => classify_load_error(e),
                        },
                        "utility" => match container.get_or_load_utility_llm().await {
                            Ok(Some(_)) => Outcome::Ready(None),
                            Ok(None) => Outcome::Skipped,
                            Err(e) => classify_load_error(e),
                        },
                        "embedding" => match container.get_or_load_embedding().await {
                            Ok(_) => Outcome::Ready(None),
                            Err(e) => classify_load_error(e),
                        },
                        _ => unreachable!(),
                    }
                })
                .await
                else {
                    return;
                };
                if cancel.is_cancelled() {
                    return;
                }
                let elapsed_ms = started.elapsed().as_millis() as u64;
                match &outcome {
                    Outcome::Ready(_) => tracing::info!(role, elapsed_ms, "prewarm: ready"),
                    Outcome::Skipped => tracing::info!(role, "prewarm: no active model — skipped"),
                    Outcome::Failed(msg) => tracing::warn!(role, error = %msg, "prewarm: failed"),
                }
                let (phase, error, degraded) = match outcome {
                    Outcome::Ready(degraded) => ("ready", None, degraded),
                    Outcome::Skipped => ("skipped", None, None),
                    Outcome::Failed(msg) => ("failed", Some(msg), None),
                };
                emit_warmup(&handle, role, phase, error, degraded);
            });
        }
    }

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
}

/// The chat's own model choice and its title, read from the row that holds
/// them. No choice is the Settings default, live.
async fn chat_model_choice(
    pool: &SqlitePool,
    conversation_id: &str,
) -> Result<(ChatModelChoice, String)> {
    let (choice, title): (Option<String>, String) =
        sqlx::query_as("SELECT model_choice, title FROM conversations WHERE id = ?")
            .bind(conversation_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(format!("Conversation {conversation_id} not found"))
            })?;
    let choice = match choice {
        None => ChatModelChoice::Default,
        Some(json) => ChatModelChoice::Explicit {
            model: serde_json::from_str(&json).map_err(|e| {
                AppError::InvalidData(format!("This chat's model choice cannot be read: {e}"))
            })?,
        },
    };
    Ok((choice, title))
}

enum Outcome {
    /// Loaded; with the reason when it runs degraded (on the CPU).
    Ready(Option<String>),
    Skipped,
    Failed(String),
}

async fn finish_or_cancel<F: std::future::Future>(
    cancel: &tokio_util::sync::CancellationToken,
    work: F,
) -> Option<F::Output> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => None,
        result = work => Some(result),
    }
}

/// First-run state (`AiModelsNotInstalled`) and router opt-out
/// (`InvalidConfig("...not configured...")`) are Skipped, not Failed —
/// the chat input mask shouldn't treat them as load errors.
///
/// `ServiceNotAvailable` carries an unusable llama-server binary; its
/// message is already user-facing, so it goes out without the prefix.
fn classify_load_error(e: AppError) -> Outcome {
    match &e {
        AppError::AiModelsNotInstalled(_) => Outcome::Skipped,
        AppError::InvalidConfig(msg) if msg.contains("not configured") => Outcome::Skipped,
        AppError::ModelLoadFailed(msg) | AppError::ServiceNotAvailable(msg) => {
            Outcome::Failed(msg.clone())
        }
        _ => Outcome::Failed(e.to_string()),
    }
}

/// `degraded` is why a model that is ready runs on the CPU, when it does.
fn emit_warmup(
    app: &tauri::AppHandle,
    role: &str,
    phase: &str,
    error: Option<String>,
    degraded: Option<String>,
) {
    use tauri::Emitter;
    let payload = serde_json::json!({
        "role": role,
        "phase": phase,
        "error": error,
        "degraded": degraded,
    });
    if let Err(e) = app.emit("model:warmup-status", payload) {
        tracing::warn!(role, phase, error = %e, "Failed to emit warmup status event");
    }
}

#[cfg(test)]
mod prewarm_tests {
    use super::finish_or_cancel;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use tokio_util::sync::CancellationToken;

    struct DropSignal(Arc<AtomicBool>);

    impl Drop for DropSignal {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn shutdown_cancels_an_in_flight_prewarm_loader() {
        let cancel = CancellationToken::new();
        let worker_cancel = cancel.clone();
        let dropped = Arc::new(AtomicBool::new(false));
        let worker_dropped = dropped.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();

        let worker = tokio::spawn(async move {
            finish_or_cancel(&worker_cancel, async move {
                let _drop_signal = DropSignal(worker_dropped);
                let _ = started_tx.send(());
                std::future::pending::<()>().await;
            })
            .await
        });

        started_rx.await.expect("loader should begin");
        cancel.cancel();

        assert_eq!(worker.await.expect("prewarm worker should join"), None);
        assert!(
            dropped.load(Ordering::SeqCst),
            "cancel drops the loader future"
        );
    }
}
