//! References Plugin — passage references saved from the reading surfaces.

use crate::features::references::commands as references;
use crate::features::references::dto::{
    CreatePassageReferenceRequestDto, PassageReferenceDto, UpdatePassageReferenceRequestDto,
};
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

#[tauri::command]
#[specta::specta]
pub async fn create_passage_reference(
    request: CreatePassageReferenceRequestDto,
    container: State<'_, Container>,
) -> Result<PassageReferenceDto, ApiError> {
    references::create_passage_reference_impl(container.inner(), request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn list_passage_references(
    limit: Option<i64>,
    container: State<'_, Container>,
) -> Result<Vec<PassageReferenceDto>, ApiError> {
    references::list_passage_references_impl(container.inner(), limit)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn update_passage_reference(
    request: UpdatePassageReferenceRequestDto,
    container: State<'_, Container>,
) -> Result<PassageReferenceDto, ApiError> {
    references::update_passage_reference_impl(container.inner(), request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_passage_reference(
    id: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    references::delete_passage_reference_impl(container.inner(), id)
        .await
        .map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("references")
        .invoke_handler(tauri::generate_handler![
            create_passage_reference,
            list_passage_references,
            update_passage_reference,
            delete_passage_reference,
        ])
        .build()
}
