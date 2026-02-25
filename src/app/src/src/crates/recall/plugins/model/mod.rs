//! Model management plugin.
//!
//! Registers model download/management and catalog/discovery commands.

pub mod commands;

use serde::Serialize;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

#[derive(Clone, Serialize)]
#[serde(tag = "type")]
pub enum DownloadEventDto {
    #[serde(rename = "started")]
    Started { id: String },
    #[serde(rename = "progress")]
    Progress {
        id: String,
        bytes_downloaded: u64,
        bytes_per_second: f64,
    },
    #[serde(rename = "paused")]
    Paused { id: String },
    #[serde(rename = "resumed")]
    Resumed { id: String },
    #[serde(rename = "completed")]
    Completed { id: String },
    #[serde(rename = "failed")]
    Failed { id: String, error: String },
    #[serde(rename = "cancelled")]
    Cancelled { id: String },
}

impl From<crate::infrastructure::services::download_manager::DownloadEvent> for DownloadEventDto {
    fn from(event: crate::infrastructure::services::download_manager::DownloadEvent) -> Self {
        use crate::infrastructure::services::download_manager::DownloadEvent;

        match event {
            DownloadEvent::Started { id } => Self::Started { id },
            DownloadEvent::Progress {
                id,
                bytes_downloaded,
                bytes_per_second,
            } => Self::Progress {
                id,
                bytes_downloaded,
                bytes_per_second,
            },
            DownloadEvent::Paused { id } => Self::Paused { id },
            DownloadEvent::Resumed { id } => Self::Resumed { id },
            DownloadEvent::Completed { id } => Self::Completed { id },
            DownloadEvent::Failed { id, error } => Self::Failed { id, error },
            DownloadEvent::Cancelled { id } => Self::Cancelled { id },
        }
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("model")
        .invoke_handler(tauri::generate_handler![
            // Download/management commands
            commands::download_model,
            commands::check_first_run_status,
            commands::download_default_embedding_model,
            commands::cancel_download,
            commands::delete_model,
            commands::list_downloaded_models,
            commands::get_download_status,
            commands::is_model_already_downloaded,
            commands::set_active_embedding_model,
            commands::set_active_inference_model,
            commands::set_active_chat_model,
            commands::get_active_chat_model,
            commands::get_active_embedding_model,
            commands::get_active_models,
            commands::clear_active_chat_model,
            commands::clear_active_embedding_model,
            commands::warm_up_active_chat_model,
            commands::validate_model_compatibility,
            commands::get_model_info,
            commands::export_model,
            commands::import_model,
            commands::refresh_model_cache,
            // Catalog/discovery commands
            crate::interfaces::commands::model_management::detect_system_capabilities,
            crate::interfaces::commands::model_management::get_compatible_models,
            crate::interfaces::commands::model_management::get_all_recommended_models,
            crate::interfaces::commands::model_management::search_model_catalog,
            crate::interfaces::commands::model_management::refresh_model_catalog,
            crate::interfaces::commands::model_management::clear_model_catalog_cache,
            crate::interfaces::commands::model_management::get_model_catalog_stats,
        ])
        .build()
}
