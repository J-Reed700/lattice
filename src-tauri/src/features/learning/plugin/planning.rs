use super::*;

#[tauri::command]
#[specta::specta]
pub async fn get_learning_plan(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningPlanDto, ApiError> {
    crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.db_pool().clone(),
    )
    .plan(&id)
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn preview_learning_curriculum_revision(
    request: PreviewLearningCurriculumRevisionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPlanDto, ApiError> {
    crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.db_pool().clone(),
    )
    .preview(&request)
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn accept_learning_curriculum_revision(
    request: LearningCurriculumRevisionActionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPlanDto, ApiError> {
    crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.db_pool().clone(),
    )
    .accept_revision(&request)
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn discard_learning_curriculum_revision(
    request: DiscardLearningCurriculumRevisionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPlanDto, ApiError> {
    crate::features::learning::curriculum_repository::LearningCurriculumRepository::new(
        container.db_pool().clone(),
    )
    .discard_revision(&request)
    .await
    .map_err(ApiError::from)
}
