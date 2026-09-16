//! Backup Plugin - Backup and export operations
//!
//! Migrated from ipc/domains/backup.rs as part of Operation Scorched Earth Batch 4
//! `delete_backup` and `get_backup_info` are not implemented.

use crate::features::backup::commands as backup;
use crate::features::backup::dto::{
    ArchiveRunDto, ArchiveSetupDto, ArchiveStatusDto, BackupInfoDto, ConfirmArchiveSetupRequestDto,
    CreateBackupResultDto, ListBackupsResultDto, RestoreArchiveRequestDto, RestoreArchiveResultDto,
    RestoreBackupResultDto, SetArchiveKeepCountRequestDto, SetArchivePassphraseRequestDto,
};
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Runtime, State,
};
use tauri_plugin_dialog::DialogExt;

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

// ---------------------------------------------------------------------------
// Encrypted off-device archive
//
// Design: docs/design/2026-09-16-encrypted-backup-archive.md.
//
// The destination folder and the restore source file are chosen by native
// dialogs **from Rust**. The webview never supplies a path, so a compromised
// renderer cannot turn "restore" into a state-injection primitive or
// "back up" into an arbitrary-write one. Same rule as `Container::exports_path`.
// ---------------------------------------------------------------------------

/// Run a native folder picker off the async runtime. `None` means cancelled.
async fn pick_folder<R: Runtime>(app: AppHandle<R>, title: &str) -> Option<std::path::PathBuf> {
    let title = title.to_string();
    tokio::task::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title(title)
            .blocking_pick_folder()
            .and_then(|picked| picked.into_path().ok())
    })
    .await
    .unwrap_or_else(|e| {
        tracing::warn!(error = %e, "folder picker task failed");
        None
    })
}

/// Run a native file picker limited to `*.lattice-backup`.
async fn pick_archive_file<R: Runtime>(app: AppHandle<R>) -> Option<std::path::PathBuf> {
    tokio::task::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Choose a Lattice backup")
            .add_filter("Lattice backup", &["lattice-backup"])
            .blocking_pick_file()
            .and_then(|picked| picked.into_path().ok())
    })
    .await
    .unwrap_or_else(|e| {
        tracing::warn!(error = %e, "file picker task failed");
        None
    })
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_get_archive_status(
    container: State<'_, Container>,
) -> Result<ArchiveStatusDto, ApiError> {
    container
        .inner()
        .archive_service()
        .status()
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_begin_archive_setup(
    container: State<'_, Container>,
) -> Result<ArchiveSetupDto, ApiError> {
    container
        .inner()
        .archive_service()
        .begin_setup()
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_confirm_archive_setup(
    request: ConfirmArchiveSetupRequestDto,
    container: State<'_, Container>,
) -> Result<ArchiveStatusDto, ApiError> {
    container
        .inner()
        .archive_service()
        .confirm_setup(request.confirmations, request.passphrase)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_choose_archive_destination<R: Runtime>(
    app: AppHandle<R>,
    container: State<'_, Container>,
) -> Result<ArchiveStatusDto, ApiError> {
    let service = container.inner().archive_service();

    // Cancelling is not an error and must not disturb what is configured.
    let Some(destination) = pick_folder(app, "Choose a backup folder").await else {
        return service.status().await.map_err(ApiError::from);
    };

    let status = service
        .set_destination(destination)
        .await
        .map_err(ApiError::from)?;

    // A destination means "back up on the schedule"; make sure the ticker is
    // running even when the local auto-backup is switched off.
    if let Err(e) = container
        .inner()
        .system
        .startup_auto_backup_use_case()
        .execute()
        .await
    {
        tracing::warn!(error = %e, "Archive destination saved but the backup schedule did not start");
    }

    Ok(status)
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_set_archive_keep_count(
    request: SetArchiveKeepCountRequestDto,
    container: State<'_, Container>,
) -> Result<ArchiveStatusDto, ApiError> {
    container
        .inner()
        .archive_service()
        .set_keep_count(request.keep_count)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_set_archive_passphrase(
    request: SetArchivePassphraseRequestDto,
    container: State<'_, Container>,
) -> Result<ArchiveStatusDto, ApiError> {
    container
        .inner()
        .archive_service()
        .set_passphrase(request.passphrase)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_rotate_recovery_code(
    container: State<'_, Container>,
) -> Result<ArchiveSetupDto, ApiError> {
    container
        .inner()
        .archive_service()
        .rotate_recovery_code()
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_disable_archive(
    container: State<'_, Container>,
) -> Result<ArchiveStatusDto, ApiError> {
    container
        .inner()
        .archive_service()
        .disable()
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn plugin_create_archive_now(
    container: State<'_, Container>,
) -> Result<ArchiveRunDto, ApiError> {
    backup::create_archive_impl(container.inner())
        .await
        .map_err(ApiError::from)
}

/// Restore is a conversation, not one call: the first attempt often comes
/// back `needs_secret`, and the UI then re-invokes with the passphrase. The
/// file the user already chose is remembered for the duration, so the picker
/// opens once and only once per restore.
#[tauri::command]
#[specta::specta]
pub async fn plugin_restore_archive<R: Runtime>(
    request: RestoreArchiveRequestDto,
    app: AppHandle<R>,
    container: State<'_, Container>,
) -> Result<RestoreArchiveResultDto, ApiError> {
    let service = container.inner().archive_service();

    let source = match service.remembered_restore_source().await {
        Some(remembered) => remembered,
        None => {
            let Some(picked) = pick_archive_file(app).await else {
                return Ok(backup::cancelled_restore());
            };
            service.remember_restore_source(picked.clone()).await;
            picked
        }
    };

    let result = backup::restore_archive_impl(source, request.secret, container.inner()).await;

    if backup::restore_attempt_is_finished(&result) {
        service.forget_restore_source().await;
    }

    result.map_err(backup::restore_api_error)
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
            plugin_get_archive_status,
            plugin_begin_archive_setup,
            plugin_confirm_archive_setup,
            plugin_choose_archive_destination,
            plugin_set_archive_keep_count,
            plugin_set_archive_passphrase,
            plugin_rotate_recovery_code,
            plugin_disable_archive,
            plugin_create_archive_now,
            plugin_restore_archive,
        ])
        .build()
}
