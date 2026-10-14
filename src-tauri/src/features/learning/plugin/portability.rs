use super::*;

/// The pack workspace lives under the app data directory; the container is
/// only the construction site, never threaded through the repository.
fn pack_repository(
    container: &Container,
) -> crate::features::learning::pack_repository::LearningPackRepository {
    crate::features::learning::pack_repository::LearningPackRepository::new(
        container.db_pool().clone(),
        container.core.data_dir().join("learning-packs"),
    )
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_portability_workspace(
    program_id: String,
    container: State<'_, Container>,
) -> Result<LearningPortabilityWorkspaceDto, ApiError> {
    pack_repository(&container)
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
    pack_repository(&container)
        .export(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn preview_learning_pack_import(
    request: PreviewLearningPackImportRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPortabilityWorkspaceDto, ApiError> {
    pack_repository(&container)
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
    pack_repository(&container)
        .apply(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_learning_pack_import_preview(
    request: CancelLearningPackImportPreviewRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPortabilityWorkspaceDto, ApiError> {
    pack_repository(&container)
        .cancel(&request)
        .await
        .map_err(ApiError::from)
}
