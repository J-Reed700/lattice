use super::*;
use crate::features::learning::course_plan::{
    self, ChangeCoursePlanEntriesRequestDto, CoursePlanViewDto, EditCoursePlanEntryRequestDto,
    PlanLearningCourseRequestDto, UndoCoursePlanEntryRequestDto,
};

#[tauri::command]
#[specta::specta]
pub async fn get_learning_course_plan(
    program_id: String,
    container: State<'_, Container>,
) -> Result<CoursePlanViewDto, ApiError> {
    course_plan::view(container.db_pool(), &program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn plan_learning_course(
    request: PlanLearningCourseRequestDto,
    container: State<'_, Container>,
) -> Result<CoursePlanViewDto, ApiError> {
    if let Some(id) = course_plan::queue(container.db_pool(), &request.program_id)
        .await
        .map_err(ApiError::from)?
    {
        container.jobs().submitted(&id).await;
    }
    course_plan::view(container.db_pool(), &request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn edit_learning_course_plan_entry(
    request: EditCoursePlanEntryRequestDto,
    container: State<'_, Container>,
) -> Result<CoursePlanViewDto, ApiError> {
    course_plan::edit_entry(container.db_pool(), request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn change_learning_course_plan_entries(
    request: ChangeCoursePlanEntriesRequestDto,
    container: State<'_, Container>,
) -> Result<CoursePlanViewDto, ApiError> {
    course_plan::check_change(container.db_pool(), &request)
        .await
        .map_err(ApiError::from)?;
    let pin = learning_model(
        &container,
        model_requirements::course_plan(),
        "Changing the planned topics",
    )
    .await?;
    course_plan::change_entries(container.db_pool(), pin.llm.as_ref(), request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn undo_learning_course_plan_entry(
    request: UndoCoursePlanEntryRequestDto,
    container: State<'_, Container>,
) -> Result<CoursePlanViewDto, ApiError> {
    course_plan::undo_entry(container.db_pool(), request)
        .await
        .map_err(ApiError::from)
}
