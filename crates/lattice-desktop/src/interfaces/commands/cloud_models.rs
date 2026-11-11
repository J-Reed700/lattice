use lattice_core::{
    application::contracts::settings::ModelWindowDto,
    application::ports::llm_port::RemoteServer,
    domain::models::model_ref::{CloudProvider, ModelRef},
    shared::ipc::ApiError,
};
use tauri::State;

use crate::features::credentials::remote_auth;
use crate::features::llm::cloud::catalog::{self, CloudModelCatalogDto};
use crate::features::llm::cloud::limits;
use crate::features::llm::engine::factory::reported_ollama_window;
use crate::interfaces::di::Container;

/// Models available to a saved cloud credential. Never returns the key.
#[tauri::command]
#[specta::specta]
pub async fn list_cloud_models(
    container: State<'_, Container>,
    provider: CloudProvider,
) -> Result<CloudModelCatalogDto, ApiError> {
    catalog::list_models(container.credentials().as_ref(), provider)
        .await
        .map_err(ApiError::from)
}

/// What `model`'s provider reports it can take: the top of its window and
/// output settings. Empty for a limit its provider does not state.
#[tauri::command]
#[specta::specta]
pub async fn get_model_limits(
    container: State<'_, Container>,
    model: ModelRef,
) -> Result<ModelWindowDto, ApiError> {
    match model {
        ModelRef::Cloud { provider, model } => {
            let Some(key) = catalog::saved_key(container.credentials().as_ref(), provider).await?
            else {
                return Ok(ModelWindowDto::default());
            };
            limits::report(provider, &model, &key)
                .await
                .map_err(ApiError::from)
        }
        ModelRef::Ollama { model } => {
            let settings = container.get_settings_use_case().execute().await?;
            let auth_header = remote_auth::header(
                container.credentials().as_ref(),
                RemoteServer::Ollama,
                &settings.llm.ollama_auth_header_name,
            )
            .await?;
            reported_ollama_window(&settings.llm, auth_header, &model)
                .await
                .map_err(ApiError::from)
        }
        ModelRef::Local { .. } | ModelRef::LlamaCpp { .. } => Ok(ModelWindowDto::default()),
    }
}
