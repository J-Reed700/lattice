//! Mention Plugin - Document mention management
//!
//! Provides commands for extracting, searching, and managing mentions (entity references) in documents.

use crate::features::mentions::dto::{
    BacklinksResultDto, ExtractMentionsResultDto, GetMentionsForDocumentResultDto, MentionDto,
    SearchMentionsResultDto,
};
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

#[tauri::command]
#[specta::specta]
pub async fn extract_mentions(
    document_id: String,
    content: String,
    container: State<'_, Container>,
) -> Result<ExtractMentionsResultDto, ApiError> {
    let use_case = container.extract_mentions_use_case();
    use_case
        .execute(document_id, content)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn search_mentions(
    query: String,
    limit: Option<i64>,
    container: State<'_, Container>,
) -> Result<SearchMentionsResultDto, ApiError> {
    let use_case = container.search_mentions_use_case();
    use_case.execute(query, limit).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_mentions_for_document(
    document_id: String,
    container: State<'_, Container>,
) -> Result<GetMentionsForDocumentResultDto, ApiError> {
    let use_case = container.get_mentions_for_document_use_case();
    use_case.execute(document_id).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_backlinks_for_mention(
    mention_name: String,
    container: State<'_, Container>,
) -> Result<BacklinksResultDto, ApiError> {
    let use_case = container.get_backlinks_use_case();
    use_case.execute(mention_name).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_mentions_by_type(
    mention_type: String,
    container: State<'_, Container>,
) -> Result<Vec<MentionDto>, ApiError> {
    let use_case = container.get_mentions_by_type_use_case();
    use_case.execute(mention_type).await.map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn create_mention(
    name: String,
    mention_type: String,
    metadata: Option<String>,
    container: State<'_, Container>,
) -> Result<MentionDto, ApiError> {
    let use_case = container.create_mention_use_case();
    use_case
        .execute(name, mention_type, metadata)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_mention(
    mention_id: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    let use_case = container.delete_mention_use_case();
    use_case.execute(mention_id).await.map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("mention")
        .invoke_handler(tauri::generate_handler![
            extract_mentions,
            search_mentions,
            get_mentions_for_document,
            get_backlinks_for_mention,
            get_mentions_by_type,
            create_mention,
            delete_mention,
        ])
        .build()
}
