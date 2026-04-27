//! LLM / model-management feature dependency injection.

use std::sync::{Arc, RwLock};

use sqlx::SqlitePool;

use crate::application::ports::{CredentialsPort, FileSystemPort, LLMPort, ModelCatalogPort,
    ModelStoragePort, SystemInfoPort};
use crate::domain::ports::file_access::{ChecksumService, FileSystemAccess};
use crate::domain::repositories::UnitOfWorkFactory;
use crate::features::download::manager::{DownloadManager, DownloadManagerService};
use crate::features::download::engine::HttpDownloadEngine;
use crate::features::download::download_repository::SqliteDownloadRepository;
use crate::features::llm::use_cases::{
    CheckModelDownloadedUseCase, DeleteModelUseCase, DownloadModelUseCase, GetAvailableModelsUseCase,
    GetBestModelUseCase, GetModelPathUseCase, GetRecommendedModelsUseCase,
    GetSystemCapabilitiesUseCase, ListDownloadedModelsUseCase,
};
use crate::features::model_management::cache_adapter::ModelCacheAdapter;
use crate::features::model_management::huggingface_adapter::HuggingFaceAdapter;
use crate::infrastructure::adapters::fs::tokio_checksum::TokioChecksumAdapter;
use crate::infrastructure::adapters::fs::TokioFileSystemAdapter;
use crate::infrastructure::llm::inference::InferenceEngine;
use crate::infrastructure::file_system::FileSystemAdapter;
use crate::infrastructure::persistence::database::DatabaseConnection;
use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::infrastructure::persistence::repositories::unit_of_work::SqliteUnitOfWorkFactory;
use crate::infrastructure::system_info_adapter::SystemInfoAdapter;
use crate::shared::error::Result;
use std::path::PathBuf;

#[derive(Clone)]
pub struct LlmDi {
    // Use cases
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
    pub llm_cache: Arc<RwLock<Option<Arc<dyn LLMPort>>>>,
    pub inference_engine_cache: Arc<RwLock<Option<Arc<InferenceEngine>>>>,
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
    llm_cache: Arc<RwLock<Option<Arc<dyn LLMPort>>>>,
    inference_engine_cache: Arc<RwLock<Option<Arc<InferenceEngine>>>>,
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
    let uow_factory =
        Arc::new(SqliteUnitOfWorkFactory::new(db_pool)) as Arc<dyn UnitOfWorkFactory>;

    let model_storage = downloaded_model_repo.clone() as Arc<dyn ModelStoragePort>;

    // Use cases
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
    let delete_model_use_case = Arc::new(DeleteModelUseCase::new(model_storage.clone()));
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
        llm_cache,
        inference_engine_cache,
        downloaded_model_repo,
        model_catalog,
        model_catalog_cache,
        download_manager,
    })
}
