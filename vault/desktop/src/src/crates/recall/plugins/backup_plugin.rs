//! Backup Plugin - Backup and export operations
//!
//! Migrated from ipc/domains/backup.rs as part of Operation Scorched Earth Batch 4
//! NOTE: delete_backup and get_backup_info NOT implemented per zen-architect spec

use crate::interfaces::commands::backup;
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

// Local DTOs for backup operations (using existing DTO structs)
use crate::application::dtos::backup_dto::{
    BackupInfoDto, CreateBackupResultDto, ListBackupsResultDto, RestoreBackupResultDto,
};

// Request DTOs defined locally
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CreateBackupRequestDto {
    pub backup_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RestoreBackupRequestDto {
    pub backup_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportMarkdownRequestDto {
    pub output_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportJsonRequestDto {
    pub output_path: String,
    pub pretty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportCsvRequestDto {
    pub output_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportHtmlRequestDto {
    pub output_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportResultDto {
    pub count: usize,
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_create_backup(
    request: CreateBackupRequestDto,
    container: State<'_, Container>,
) -> Result<CreateBackupResultDto, ApiError> {
    let backup_path = backup::create_backup_impl(request.backup_path, container.inner())
        .await
        .map_err(ApiError::from)?;

    Ok(CreateBackupResultDto {
        backup_path,
        size: 0,
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_restore_backup(
    request: RestoreBackupRequestDto,
    container: State<'_, Container>,
) -> Result<RestoreBackupResultDto, ApiError> {
    backup::restore_backup_impl(request.backup_path, &container)
        .await
        .map_err(ApiError::from)?;

    Ok(RestoreBackupResultDto {
        success: true,
        restored_count: 0,
        message: Some("Backup restored successfully".to_string()),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_list_backups(
    container: State<'_, Container>,
) -> Result<ListBackupsResultDto, ApiError> {
    let data_dir = container.inner().core.data_dir().to_path_buf();

    let backups = backup::list_backups_impl(data_dir)
        .await
        .map_err(ApiError::from)?;

    Ok(ListBackupsResultDto {
        backups: backups
            .into_iter()
            .map(|b| BackupInfoDto {
                path: b.path,
                name: b.name,
                created_at: b.created_at,
                version: b.version,
                file_count: b.file_count,
                size: b.size,
            })
            .collect(),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_export_markdown(
    request: ExportMarkdownRequestDto,
    container: State<'_, Container>,
) -> Result<ExportResultDto, ApiError> {
    let count = backup::export_markdown_impl(request.output_dir, container.inner())
        .await
        .map_err(ApiError::from)?;

    Ok(ExportResultDto { count })
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_export_json(
    request: ExportJsonRequestDto,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    backup::export_json_impl(request.output_path, request.pretty, container.inner())
        .await
        .map_err(ApiError::from)?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_export_csv(
    request: ExportCsvRequestDto,
    container: State<'_, Container>,
) -> Result<ExportResultDto, ApiError> {
    let count = backup::export_csv_impl(request.output_path, container.inner())
        .await
        .map_err(ApiError::from)?;

    Ok(ExportResultDto { count })
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_export_html(
    request: ExportHtmlRequestDto,
    container: State<'_, Container>,
) -> Result<ExportResultDto, ApiError> {
    let count = backup::export_html_impl(request.output_dir, container.inner())
        .await
        .map_err(ApiError::from)?;

    Ok(ExportResultDto { count })
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("backup")
        .invoke_handler(tauri::generate_handler![
            plugin_create_backup,
            plugin_restore_backup,
            plugin_list_backups,
            plugin_export_markdown,
            plugin_export_json,
            plugin_export_csv,
            plugin_export_html,
        ])
        .build()
}
