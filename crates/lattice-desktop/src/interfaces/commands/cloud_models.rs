use lattice_core::{domain::models::model_ref::CloudProvider, shared::ipc::ApiError};

use crate::features::llm::cloud::catalog::{self, CloudModelCatalogDto};

/// Models available to a saved cloud credential. Never returns the key.
#[tauri::command]
#[specta::specta]
pub async fn list_cloud_models(provider: CloudProvider) -> Result<CloudModelCatalogDto, ApiError> {
    catalog::list_models(provider).await.map_err(ApiError::from)
}
