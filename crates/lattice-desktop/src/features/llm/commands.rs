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

/// Catalog models suited to the system's hardware, optionally filtered by
/// performance tier.
///
/// # Errors
///
/// * `AppError::Other` - Failed to detect system capabilities or generate recommendations
pub async fn get_recommended_models(
    tier: Option<PerformanceTier>,
    container: State<'_, Container>,
) -> Result<RecommendedModelsDto, AppError> {
    let use_case = container.get_recommended_models_use_case();
    let request = GetRecommendationsRequestDto { tier };
    use_case.execute(request).await
}

/// The single best model for the system's hardware.
///
/// # Errors
///
/// * `AppError::Other` - Failed to detect capabilities or no suitable model found
pub async fn get_best_model(container: State<'_, Container>) -> Result<BestModelDto, AppError> {
    let use_case = container.get_best_model_use_case();
    use_case.execute().await
}

#[cfg(test)]
mod tests;
