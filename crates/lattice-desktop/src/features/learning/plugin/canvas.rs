use super::*;

#[tauri::command]
#[specta::specta]
pub async fn get_learning_canvas_workspace(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningCanvasWorkspaceDto, ApiError> {
    crate::features::learning::canvas_repository::LearningCanvasRepository::new(
        container.db_pool().clone(),
    )
    .workspace(&id)
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn create_learning_canvas(
    request: CreateLearningCanvasRequestDto,
    container: State<'_, Container>,
) -> Result<LearningCanvasWorkspaceDto, ApiError> {
    let scene = crate::features::learning::canvas_repository::validate_scene(&request.scene_json)
        .map_err(ApiError::from)?;
    let repo = crate::features::learning::canvas_repository::LearningCanvasRepository::new(
        container.db_pool().clone(),
    );
    repo.create(&request, &scene)
        .await
        .map_err(ApiError::from)?;
    repo.workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn save_learning_canvas(
    request: SaveLearningCanvasRequestDto,
    container: State<'_, Container>,
) -> Result<LearningCanvasWorkspaceDto, ApiError> {
    let scene = crate::features::learning::canvas_repository::validate_scene(&request.scene_json)
        .map_err(ApiError::from)?;
    let repo = crate::features::learning::canvas_repository::LearningCanvasRepository::new(
        container.db_pool().clone(),
    );
    repo.save(&request, &scene).await.map_err(ApiError::from)?;
    repo.workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn create_learning_canvas_snapshot(
    request: CreateLearningCanvasSnapshotRequestDto,
    container: State<'_, Container>,
) -> Result<LearningCanvasWorkspaceDto, ApiError> {
    let repo = crate::features::learning::canvas_repository::LearningCanvasRepository::new(
        container.db_pool().clone(),
    );
    repo.create_snapshot(&request)
        .await
        .map_err(ApiError::from)?;
    repo.workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn restore_learning_canvas_snapshot(
    request: RestoreLearningCanvasSnapshotRequestDto,
    container: State<'_, Container>,
) -> Result<LearningCanvasWorkspaceDto, ApiError> {
    let repo = crate::features::learning::canvas_repository::LearningCanvasRepository::new(
        container.db_pool().clone(),
    );
    repo.restore(&request).await.map_err(ApiError::from)?;
    repo.workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}
