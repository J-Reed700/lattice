//! Tauri commands for the vault. Settings live in the generic settings
//! plugin; writer/watcher run in the background.

use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

use crate::features::vault::events::VaultConflictDto;
use crate::features::vault::watcher::RescanSummary;
use crate::interfaces::di::Container;
use lattice_core::shared::ipc::{ApiError, ErrorCode};

#[tauri::command]
#[specta::specta]
pub async fn rescan_vault(container: State<'_, Container>) -> Result<RescanSummary, ApiError> {
    container.rescan_vault().await.map_err(|e| ApiError {
        code: ErrorCode::InternalError,
        message: e.to_string(),
        details: None,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn list_vault_conflicts(
    container: State<'_, Container>,
) -> Result<Vec<VaultConflictDto>, ApiError> {
    container
        .list_vault_conflicts()
        .await
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e.to_string(),
            details: None,
        })
}

#[tauri::command]
#[specta::specta]
pub async fn dismiss_vault_conflicts(
    container: State<'_, Container>,
    ids: Vec<String>,
) -> Result<(), ApiError> {
    container
        .dismiss_vault_conflicts(ids)
        .await
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e.to_string(),
            details: None,
        })
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("vault")
        .invoke_handler(tauri::generate_handler![
            rescan_vault,
            list_vault_conflicts,
            dismiss_vault_conflicts
        ])
        .build()
}
