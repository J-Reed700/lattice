use super::*;

#[tauri::command]
#[specta::specta]
pub async fn get_learning_portability_workspace(
    program_id: String,
    container: State<'_, Container>,
) -> Result<LearningPortabilityWorkspaceDto, ApiError> {
    crate::features::learning::pack_repository::LearningPackRepository::new(
        container.db_pool().clone(),
    )
    .workspace(&program_id)
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn export_learning_pack(
    request: ExportLearningPackRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPortabilityWorkspaceDto, ApiError> {
    crate::features::learning::pack_repository::LearningPackRepository::new(
        container.db_pool().clone(),
    )
    .export(&container, &request)
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn preview_learning_pack_import(
    request: PreviewLearningPackImportRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPortabilityWorkspaceDto, ApiError> {
    crate::features::learning::pack_repository::LearningPackRepository::new(
        container.db_pool().clone(),
    )
    .preview(&request)
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn apply_learning_pack_import(
    request: ApplyLearningPackImportRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPortabilityWorkspaceDto, ApiError> {
    crate::features::learning::pack_repository::LearningPackRepository::new(
        container.db_pool().clone(),
    )
    .apply(&container, &request)
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_learning_pack_import_preview(
    request: CancelLearningPackImportPreviewRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPortabilityWorkspaceDto, ApiError> {
    crate::features::learning::pack_repository::LearningPackRepository::new(
        container.db_pool().clone(),
    )
    .cancel(&request)
    .await
    .map_err(ApiError::from)
}
