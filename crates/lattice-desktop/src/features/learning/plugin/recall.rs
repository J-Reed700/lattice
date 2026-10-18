use super::*;

#[tauri::command]
#[specta::specta]
pub async fn get_learning_recall_workspace(
    program_id: String,
    container: State<'_, Container>,
) -> Result<LearningRecallWorkspaceDto, ApiError> {
    crate::features::learning::recall_repository::LearningRecallRepository::new(
        container.db_pool().clone(),
    )
    .workspace(&program_id)
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn save_learning_recall_card(
    request: SaveLearningRecallCardRequestDto,
    container: State<'_, Container>,
) -> Result<LearningRecallWorkspaceDto, ApiError> {
    crate::features::learning::recall_repository::LearningRecallRepository::new(
        container.db_pool().clone(),
    )
    .save_card(&request)
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn decide_learning_recall_duplicate(
    request: DecideLearningRecallDuplicateRequestDto,
    container: State<'_, Container>,
) -> Result<LearningRecallWorkspaceDto, ApiError> {
    crate::features::learning::recall_repository::LearningRecallRepository::new(
        container.db_pool().clone(),
    )
    .decide_duplicate(&request)
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn review_learning_recall_card(
    request: ReviewLearningRecallCardRequestDto,
    container: State<'_, Container>,
) -> Result<LearningRecallWorkspaceDto, ApiError> {
    crate::features::learning::recall_repository::LearningRecallRepository::new(
        container.db_pool().clone(),
    )
    .review_card(&request)
    .await
    .map_err(ApiError::from)
}
