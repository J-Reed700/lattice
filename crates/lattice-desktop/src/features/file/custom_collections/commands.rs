use crate::features::file::custom_collections::dto::{
    CreateCustomCollectionRequest, CustomCollectionDto,
};
use crate::features::file::custom_collections::repository::CustomCollectionsRepository;
use crate::interfaces::di::Container;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub async fn list_custom_collections(
    container: State<'_, Container>,
) -> std::result::Result<Vec<CustomCollectionDto>, String> {
    CustomCollectionsRepository::new(container.db_pool().clone())
        .list()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn create_custom_collection(
    request: CreateCustomCollectionRequest,
    container: State<'_, Container>,
) -> std::result::Result<String, String> {
    CustomCollectionsRepository::new(container.db_pool().clone())
        .create(request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn rename_custom_collection(
    collection_id: String,
    name: String,
    container: State<'_, Container>,
) -> std::result::Result<(), String> {
    CustomCollectionsRepository::new(container.db_pool().clone())
        .rename(&collection_id, &name)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn move_custom_collection(
    collection_id: String,
    parent_id: Option<String>,
    container: State<'_, Container>,
) -> std::result::Result<(), String> {
    CustomCollectionsRepository::new(container.db_pool().clone())
        .move_to(&collection_id, parent_id.as_deref())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_custom_collection(
    collection_id: String,
    container: State<'_, Container>,
) -> std::result::Result<(), String> {
    CustomCollectionsRepository::new(container.db_pool().clone())
        .delete(&collection_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn add_documents_to_custom_collection(
    collection_id: String,
    document_ids: Vec<String>,
    container: State<'_, Container>,
) -> std::result::Result<(), String> {
    CustomCollectionsRepository::new(container.db_pool().clone())
        .add_documents(&collection_id, &document_ids)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn remove_documents_from_custom_collection(
    collection_id: String,
    document_ids: Vec<String>,
    container: State<'_, Container>,
) -> std::result::Result<(), String> {
    CustomCollectionsRepository::new(container.db_pool().clone())
        .remove_documents(&collection_id, &document_ids)
        .await
        .map_err(|error| error.to_string())
}
