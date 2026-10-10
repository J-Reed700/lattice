use super::*;
use crate::features::learning::recall::{
    study_dto::*, study_repository::StudyRepository, study_service as service,
};

#[tauri::command]
#[specta::specta]
pub async fn list_study_decks(
    container: State<'_, Container>,
) -> Result<Vec<StudyDeckSummaryDto>, ApiError> {
    StudyRepository::new(container.db_pool().clone())
        .list(chrono::Utc::now().timestamp_millis())
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn get_study_deck(
    id: String,
    container: State<'_, Container>,
) -> Result<StudyDeckDto, ApiError> {
    StudyRepository::new(container.db_pool().clone())
        .get(&id)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn generate_study_deck(
    request: GenerateStudyDeckRequestDto,
    container: State<'_, Container>,
) -> Result<StudyDeckDto, ApiError> {
    service::validate_generation(&request).map_err(ApiError::from)?;
    let llm = container.get_or_load_llm().await.map_err(ApiError::from)?;
    service::generate_deck(
        &StudyRepository::new(container.db_pool().clone()),
        container.library_passages().as_ref(),
        llm.as_ref(),
        request,
    )
    .await
    .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn generate_conversation_study_deck(
    request: GenerateConversationStudyDeckRequestDto,
    container: State<'_, Container>,
) -> Result<StudyDeckDto, ApiError> {
    let llm = container.get_or_load_llm().await.map_err(ApiError::from)?;
    service::generate_conversation_deck(
        &StudyRepository::new(container.db_pool().clone()),
        llm.as_ref(),
        request,
    )
    .await
    .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn review_study_card(
    request: ReviewStudyCardRequestDto,
    container: State<'_, Container>,
) -> Result<StudyCardDto, ApiError> {
    service::review(&StudyRepository::new(container.db_pool().clone()), request)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn update_study_card(
    request: UpdateStudyCardRequestDto,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    service::update_card(&StudyRepository::new(container.db_pool().clone()), request)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn delete_study_deck(
    id: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    StudyRepository::new(container.db_pool().clone())
        .delete_deck(&id)
        .await
        .map_err(ApiError::from)
}
