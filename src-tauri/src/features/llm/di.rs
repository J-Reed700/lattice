//! LLM / model-management feature dependency injection.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::application::ports::UnitOfWorkFactory;
use crate::application::ports::{
    CredentialsPort, FileSystemPort, LLMPort, ModelCatalogPort, ModelStoragePort, SystemInfoPort,
};
use crate::domain::ports::file_access::{ChecksumService, FileSystemAccess};
use crate::domain::repositories::downloaded_model_repository::DownloadedModelRepository as DownloadedModelRepositoryPort;
use crate::features::download::download_repository::SqliteDownloadRepository;
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
use crate::infrastructure::file_system::FileSystemAdapter;
use crate::infrastructure::persistence::database::DatabaseConnection;
use crate::infrastructure::persistence::repositories::unit_of_work::SqliteUnitOfWorkFactory;
use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::infrastructure::system_info_adapter::SystemInfoAdapter;
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use std::path::PathBuf;

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
) -> Result<LlmDi> {
    // Model catalog (HuggingFace + SQLite cache)
    let downloaded_model_repo = Arc::new(DownloadedModelRepository::new(db_pool.clone()));

    let huggingface = Arc::new(HuggingFaceAdapter::new()) as Arc<dyn ModelCatalogPort>;
    let model_catalog_cache = Arc::new(ModelCacheAdapter::new(db_pool.clone(), huggingface).await?);
    let model_catalog = model_catalog_cache.clone() as Arc<dyn ModelCatalogPort>;

    let system_info = Arc::new(SystemInfoAdapter::new()) as Arc<dyn SystemInfoPort>;

    // Download wiring
    let download_repository = Arc::new(SqliteDownloadRepository::new(db_conn));
    let download_engine = Arc::new(HttpDownloadEngine::new()?);
    let download_manager = Arc::new(DownloadManagerService::new(
        download_repository,
        download_engine,
        data_dir,
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
/// Model loading, the per-role caches and their invalidation live here; the
/// `Container` only owns the cache handles and the `AppHandle` the sidecar
/// factory needs.
impl Container {
    /// Returns the stored AppHandle, if any. Used by the LLM factory
    /// to populate `LLMConfig::Local.app_handle` when the sidecar
    /// feature is enabled.
    fn app_handle(&self) -> Option<tauri::AppHandle> {
        self.app_handle.clone()
    }

    fn model_loader(&self) -> crate::infrastructure::model_loading::ModelLoader {
        crate::infrastructure::model_loading::ModelLoader::new(
            self.ai.downloaded_model_repo().clone(),
            self.app_handle(),
        )
    }

    async fn load_llm_with_fallback(&self) -> Result<Arc<dyn LLMPort>> {
        let settings = self
            .system
            .get_settings_use_case()
            .execute()
            .await
            .map_err(|e| AppError::InvalidConfig(format!("Failed to load settings: {}", e)))?;
        self.model_loader().load(&settings.llm).await
    }

    /// Gets the LLM service, loading it lazily if needed.
    ///
    /// Coalesces concurrent misses and prevents invalidated loads from
    /// publishing into the current cache generation.
    pub async fn get_or_load_llm(&self) -> Result<Arc<dyn LLMPort>> {
        let generation = self.llm_cache.generation();
        self.llm_cache
            .get_or_load((), generation, || async {
                self.load_llm_with_fallback().await.map(Some)
            })
            .await?
            .ok_or_else(|| AppError::AiModelsNotInstalled("Chat model unavailable".into()))
    }

    /// Gets the router LLM service, loading it lazily if needed.
    ///
    /// Uses a separate cache so the router can use a smaller model.
    pub async fn get_or_load_router_llm(&self) -> Result<Arc<dyn LLMPort>> {
        let generation = self.router_llm_cache.generation();
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

        self.router_llm_cache
            .get_or_load(model_name.clone(), generation, || async {
                self.model_loader()
                    .load_router_llm(&settings.llm, &model_name)
                    .await
                    .map(Some)
            })
            .await?
            .ok_or_else(|| AppError::AiModelsNotInstalled("Router model unavailable".into()))
    }

    /// Get the utility LLM if one is configured.
    pub async fn get_or_load_utility_llm(&self) -> Result<Option<Arc<dyn LLMPort>>> {
        let generation = self.utility_llm_cache.generation();
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

        let generation_config = crate::features::llm::engine::GenerationConfig {
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

        self.utility_llm_cache
            .get_or_load(cache_key, generation, || async {
                if active.location().is_local() {
                    self.model_loader()
                        .load_utility_local(
                            &active,
                            generation_config,
                            settings.llm.local_context_window,
                        )
                        .await
                } else {
                    self.model_loader()
                        .load_utility_remote(
                            &settings.llm,
                            &ollama_utility_tag,
                            generation_config,
                        )
                        .await
                        .or_else(|e| {
                            tracing::warn!(
                                model_id = %active.model_id(),
                                utility_model = %ollama_utility_tag,
                                error = %e,
                                "Failed to reach the remote utility model — falling back to chat LLM"
                            );
                            Ok(None)
                        })
                }
            })
            .await
    }

    /// Invalidate the LLM cache
    ///
    /// Call this when the active model changes to force reload on next access
    pub fn invalidate_llm_cache(&self) {
        self.llm_cache.invalidate();
    }

    /// Invalidate the router LLM cache.
    pub fn invalidate_router_llm_cache(&self) {
        self.router_llm_cache.invalidate();
    }

    pub fn invalidate_utility_llm_cache(&self) {
        self.utility_llm_cache.invalidate();
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
        crate::features::llm::engine::sidecar_manager::spawn_binary_preflight(&app_handle);
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

enum Outcome {
    Ready,
    Skipped,
    Failed(String),
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
