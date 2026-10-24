//! Commands over installed models: download status, paths, listing and deletion.

use std::fs;

use tauri::{Manager, State};

use crate::features::llm::dto::*;
use crate::interfaces::di::Container;
use lattice_core::shared::error::AppError;

pub async fn is_model_downloaded(
    model_id: String,
    container: State<'_, Container>,
) -> Result<CheckModelDownloadedResponseDto, AppError> {
    let use_case = container.check_model_downloaded_use_case();
    let request = CheckModelDownloadedRequestDto { model_id };
    use_case.execute(request).await
}

pub async fn get_model_path(
    model_id: String,
    container: State<'_, Container>,
) -> Result<GetModelPathResponseDto, AppError> {
    let use_case = container.get_model_path_use_case();
    let request = GetModelPathRequestDto { model_id };
    use_case.execute(request).await
}

pub async fn delete_model(
    model_id: String,
    container: State<'_, Container>,
) -> Result<DeleteModelResponseDto, AppError> {
    let use_case = container.delete_model_use_case();
    let request = DeleteModelRequestDto {
        model_id: model_id.clone(),
    };
    let response = use_case.execute(request).await?;

    Ok(response)
}

pub async fn list_models(
    container: State<'_, Container>,
) -> Result<Vec<DownloadedModelDto>, AppError> {
    let use_case = container.list_models_use_case();
    let result = use_case.execute().await?;
    Ok(result.models)
}

pub async fn get_model_download_path(app_handle: tauri::AppHandle) -> Result<String, AppError> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| AppError::FileSystem(format!("Failed to get app data directory: {}", e)))?;

    let models_dir = app_data_dir.join("models");

    fs::create_dir_all(&models_dir)
        .map_err(|e| AppError::FileSystem(format!("Failed to create models directory: {}", e)))?;

    let canonical_models = models_dir
        .canonicalize()
        .map_err(|e| AppError::FileSystem(format!("Failed to canonicalize models path: {}", e)))?;

    let canonical_app_data = app_data_dir.canonicalize().map_err(|e| {
        AppError::FileSystem(format!("Failed to canonicalize app data path: {}", e))
    })?;

    if !canonical_models.starts_with(&canonical_app_data) {
        return Err(AppError::Security(
            "Models directory is not within app data directory".to_string(),
        ));
    }

    canonical_models
        .to_str()
        .ok_or_else(|| AppError::FileSystem("Failed to convert path to string".to_string()))
        .map(|s| s.to_string())
}
