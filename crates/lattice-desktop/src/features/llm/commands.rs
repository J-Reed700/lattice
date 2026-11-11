//! LLM management commands for local inference.
use crate::features::llm::dto::*;
use crate::interfaces::di::Container;
use lattice_core::shared::error::AppError;
use tauri::State;

mod installed;

pub use installed::{
    delete_model, get_model_download_path, get_model_path, is_model_downloaded, list_models,
};

/// The system's CPU, RAM, GPU and OS capabilities, to suggest local models it
/// can run.
///
/// # Errors
///
/// * `AppError::Other` - Failed to detect hardware capabilities
pub async fn get_system_capabilities(
    container: State<'_, Container>,
) -> Result<SystemCapabilitiesDto, AppError> {
    let use_case = container.get_system_capabilities_use_case();
    use_case.execute().await
}

/// Every LLM model in the catalog that can be downloaded for local inference.
///
/// # Errors
///
/// * `AppError::Other` - Failed to load model catalog or parse model definitions
pub async fn get_available_models(
    container: State<'_, Container>,
) -> Result<Vec<ModelInfoDto>, AppError> {
    let use_case = container.get_available_models_use_case();
    let result = use_case.execute().await?;
    Ok(result.models)
}
