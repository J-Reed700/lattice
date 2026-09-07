//! Backup Plugin - Backup and export operations
//!
//! Migrated from ipc/domains/backup.rs as part of Operation Scorched Earth Batch 4
//! NOTE: delete_backup and get_backup_info NOT implemented per zen-architect spec

use crate::features::backup::commands as backup;
use crate::features::backup::dto::{
    BackupInfoDto, CreateBackupResultDto, ListBackupsResultDto, RestoreBackupResultDto,
};
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
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
    /// `None` means the app's exports folder, which is the only place the
    /// backend will write. A supplied path is confined to that folder.
    #[serde(default)]
    pub output_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportJsonRequestDto {
    /// `None` means the app's exports folder. See `ExportMarkdownRequestDto`.
    #[serde(default)]
    pub output_path: Option<String>,
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
    /// Where the files landed, so the UI can offer "Show in Finder" without
    /// guessing.
    pub output_dir: String,
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

    // Report the real size and creation time. A failed metadata read keeps the
    // old fallbacks rather than failing the backup — the file exists either way.
    // repository-barrier-allow: reads the freshly written backup file's own metadata, not app state.
    let (size, created_at) = match tokio::fs::metadata(&backup_path).await {
        Ok(meta) => {
            let created = meta
                .modified()
                .ok()
                .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
            (meta.len(), created)
        }
        Err(e) => {
            tracing::warn!(error = %e, path = %backup_path, "backup written but metadata unreadable");
            (0, chrono::Utc::now().to_rfc3339())
        }
    };

    Ok(CreateBackupResultDto {
        backup_path,
        size,
        created_at,
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
    // `backup::list_backups_impl` scans <data_dir> for "*.lattice-backup"; backups
    // are written by BackupAdapter to <data_dir>/backups/*.db, so that scan always
    // returned []. Route through the port, which reads the right directory, matches
    // the right extension, and fills in created_at, file_count and version by
    // opening each backup.
    let data_dir = container.inner().core.data_dir().to_path_buf();

    let backups = container
        .inner()
        .backup_port()
        .list_backups(data_dir)
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
    let summary = backup::export_markdown_impl(request.output_dir, container.inner())
        .await
        .map_err(ApiError::from)?;

    Ok(ExportResultDto {
        count: summary.count(),
        output_dir: summary.output_dir,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_export_json(
    request: ExportJsonRequestDto,
    container: State<'_, Container>,
) -> Result<ExportResultDto, ApiError> {
    let summary = backup::export_json_impl(request.output_path, request.pretty, container.inner())
        .await
        .map_err(ApiError::from)?;

    Ok(ExportResultDto {
        count: summary.count(),
        output_dir: summary.output_dir,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_export_csv(
    request: ExportCsvRequestDto,
    container: State<'_, Container>,
) -> Result<ExportResultDto, ApiError> {
    let count = backup::export_csv_impl(request.output_path.clone(), container.inner())
        .await
        .map_err(ApiError::from)?;

    Ok(ExportResultDto {
        count,
        output_dir: request.output_path,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_export_html(
    request: ExportHtmlRequestDto,
    container: State<'_, Container>,
) -> Result<ExportResultDto, ApiError> {
    let count = backup::export_html_impl(request.output_dir.clone(), container.inner())
        .await
        .map_err(ApiError::from)?;

    Ok(ExportResultDto {
        count,
        output_dir: request.output_dir,
    })
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
