//! Tags Plugin - Document tagging and tag management
//!
//! Migrated from ipc/domains/tags.rs as part of Operation Scorched Earth Batch 2

use crate::features::tags::dto::{
    ApplyTagsRequestDto, ApplyTagsResponseDto, GenerateTagsRequestDto, GenerateTagsResponseDto,
    RemoveTagRequestDto, TagDto, TagWithCountDto,
};
use crate::features::tags::commands as tag_commands_full;
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

#[tauri::command]
#[specta::specta]
pub async fn get_all_tags_with_counts(
    container: State<'_, Container>,
) -> Result<Vec<TagWithCountDto>, ApiError> {
    tag_commands_full::get_all_tags_with_counts_impl(container.inner())
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_document_tags(
    document_id: String,
    container: State<'_, Container>,
) -> Result<Vec<TagDto>, ApiError> {
    tag_commands_full::get_document_tags_impl(container.inner(), document_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn apply_tags(
    request: ApplyTagsRequestDto,
    container: State<'_, Container>,
) -> Result<Vec<TagDto>, ApiError> {
    tag_commands_full::apply_tags_impl(container.inner(), request)
        .await
        .map(|response| response.tags)
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn remove_tag_from_document(
    request: RemoveTagRequestDto,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    tag_commands_full::remove_tag_from_document_impl(container.inner(), request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn generate_tags_for_document(
    request: GenerateTagsRequestDto,
    container: State<'_, Container>,
) -> Result<Vec<String>, ApiError> {
    tag_commands_full::generate_tags_impl(container.inner(), request)
        .await
        .map(|response| response.tags)
        .map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("tags")
        .invoke_handler(tauri::generate_handler![
            get_all_tags_with_counts,
            get_document_tags,
            apply_tags,
            remove_tag_from_document,
            generate_tags_for_document,
        ])
        .build()
}
