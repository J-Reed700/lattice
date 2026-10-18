//! File plugin commands.
//!
//! Connects plugin stubs to actual implementations in `interfaces::commands`.

use crate::features::file::commands as file_commands;
use crate::interfaces::commands::document_list;
use crate::interfaces::di::Container;
use lattice_core::application::contracts::recent_documents::RecentDocument;
use lattice_core::shared::error::AppError;
use lattice_core::shared::ipc::ApiError;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct IndexingStatus {
    pub active: bool,
    pub progress: f32,
}

#[tauri::command]
#[specta::specta]
pub async fn list_indexed_files(
    limit: Option<usize>,
    container: State<'_, Container>,
) -> Result<Vec<String>, ApiError> {
    let effective_limit = limit.unwrap_or(10000);
    let documents = document_list::list_all_documents(container, effective_limit)
        .await
        .map_err(ApiError::from)?;

    Ok(documents.into_iter().map(|doc| doc.file_path).collect())
}

#[tauri::command]
#[specta::specta]
pub async fn list_all_documents(
    limit: usize,
    container: State<'_, Container>,
) -> Result<Vec<document_list::DocumentMetadataDto>, ApiError> {
    document_list::list_all_documents(container, limit)
        .await
        .map_err(ApiError::from)
}

/// One row of the vault's type mix.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CorpusTypeCountDto {
    pub r#type: String,
    pub count: i64,
}

/// Vault-wide type mix and recent growth.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CorpusShapeDto {
    pub total: i64,
    pub by_type: Vec<CorpusTypeCountDto>,
    pub grown_last7_days: i64,
}

#[tauri::command]
#[specta::specta]
pub async fn get_corpus_shape(container: State<'_, Container>) -> Result<CorpusShapeDto, ApiError> {
    let shape = container
        .get_corpus_shape_use_case()
        .execute()
        .await
        .map_err(ApiError::from)?;

    Ok(CorpusShapeDto {
        total: shape.total,
        by_type: shape
            .by_type
            .into_iter()
            .map(|(label, count)| CorpusTypeCountDto {
                r#type: label,
                count,
            })
            .collect(),
        grown_last7_days: shape.grown_last_7_days,
    })
}

/// A conversation that has this document among its linked documents.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CitingConversationDto {
    pub conversation_id: String,
    pub title: String,
    pub updated_at: String,
    pub passage_count: i64,
}

#[tauri::command]
#[specta::specta]
pub async fn list_conversations_citing_document(
    document_id: String,
    limit: Option<i64>,
    container: State<'_, Container>,
) -> Result<Vec<CitingConversationDto>, ApiError> {
    let rows = container
        .list_citing_conversations_use_case()
        .execute(document_id, limit)
        .await
        .map_err(ApiError::from)?;

    Ok(rows
        .into_iter()
        .map(|row| CitingConversationDto {
            conversation_id: row.conversation_id,
            title: row.title,
            updated_at: row.updated_at,
            passage_count: row.passage_count,
        })
        .collect())
}

#[tauri::command]
#[specta::specta]
pub async fn get_file_path_by_id(
    file_id: String,
    container: State<'_, Container>,
) -> Result<String, ApiError> {
    file_commands::get_file_path_by_id(file_id, container)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_indexed_folders(
    container: State<'_, Container>,
) -> Result<Vec<file_commands::IndexedFolder>, ApiError> {
    file_commands::get_indexed_folders(container)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_indexing_activities(
    limit: usize,
    container: State<'_, Container>,
) -> Result<Vec<file_commands::IndexingActivity>, ApiError> {
    file_commands::get_indexing_activities(container, limit)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_recent_documents(
    limit: usize,
    container: State<'_, Container>,
) -> Result<Vec<RecentDocument>, ApiError> {
    let records = container
        .recent_documents_repository()
        .get_recent_documents(limit)
        .await
        .map_err(|e| AppError::Other(format!("Failed to get recent documents: {}", e)))?;
    Ok(records.into_iter().map(RecentDocument::from).collect())
}
