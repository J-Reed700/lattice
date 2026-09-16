//! Tauri commands for the vault. Settings live in the generic settings
//! plugin; writer/watcher run in the background.

use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

use crate::features::vault::watcher::RescanSummary;
use crate::interfaces::di::Container;
use crate::shared::api_result::{ApiError, ErrorCode};

#[tauri::command]
#[specta::specta]
pub async fn rescan_vault(container: State<'_, Container>) -> Result<RescanSummary, ApiError> {
    container.rescan_vault().await.map_err(|e| ApiError {
        code: ErrorCode::InternalError,
        message: e.to_string(),
        details: None,
    })
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("vault")
        .invoke_handler(tauri::generate_handler![rescan_vault])
        .build()
}
