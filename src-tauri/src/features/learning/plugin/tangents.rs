use super::*;

#[tauri::command]
#[specta::specta]
pub async fn create_learning_tangent(
    request: CreateLearningTangentRequestDto,
    container: State<'_, Container>,
) -> Result<LearningTangentDto, ApiError> {
    LearningRepository::new(container.db_pool().clone())
        .create_tangent(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn list_learning_tangents(
    request: ListLearningTangentsRequestDto,
    container: State<'_, Container>,
) -> Result<Vec<LearningTangentDto>, ApiError> {
    LearningRepository::new(container.db_pool().clone())
        .list_tangents(request.program_id.as_deref())
        .await
        .map_err(ApiError::from)
}
