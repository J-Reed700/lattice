//! Updates Plugin - Application update checking and version information
//!
//! Migrated from ipc/domains/updates.rs as part of Operation Scorched Earth Batch 4

use crate::features::updates::commands as updates_commands;
use crate::features::updates::dto::{UpdateInfoDto, VersionInfoDto};
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

#[tauri::command]
#[specta::specta]
pub async fn check_for_updates(container: State<'_, Container>) -> Result<UpdateInfoDto, ApiError> {
    updates_commands::check_for_updates_impl(container.inner())
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_version_info(container: State<'_, Container>) -> Result<VersionInfoDto, ApiError> {
    updates_commands::get_version_info_impl(container.inner())
        .await
        .map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("updates")
        .invoke_handler(tauri::generate_handler![
            check_for_updates,
            get_version_info,
        ])
        .build()
}
