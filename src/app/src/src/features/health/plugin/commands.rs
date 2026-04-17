//! Health plugin commands - system health and diagnostics

use crate::features::health::commands as health_commands;
use crate::interfaces::di::Container;
use crate::shared::api_result::{ApiError, ErrorCode};
use tauri::State;

pub use super::types::*;

#[tauri::command]
#[specta::specta]
pub async fn health_check(container: State<'_, Container>) -> Result<HealthStatus, ApiError> {
    let json_str = health_commands::health_check_impl(container.inner())
        .await
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e,
            details: None,
        })?;

    serde_json::from_str(&json_str).map_err(|e| ApiError {
        code: ErrorCode::SerializationError,
        message: format!("Failed to parse health status: {}", e),
        details: None,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn get_system_stats(container: State<'_, Container>) -> Result<SystemStats, ApiError> {
    let json_str = health_commands::get_system_stats_impl(container.inner())
        .await
        .map_err(|e| ApiError {
            code: ErrorCode::InternalError,
            message: e,
            details: None,
        })?;

    serde_json::from_str(&json_str).map_err(|e| ApiError {
        code: ErrorCode::SerializationError,
        message: format!("Failed to parse system stats: {}", e),
        details: None,
    })
}

#[tauri::command]
#[specta::specta]
pub fn get_version() -> Result<String, ApiError> {
    Ok(health_commands::get_version())
}

#[tauri::command]
#[specta::specta]
pub async fn initialize_database(container: State<'_, Container>) -> Result<String, ApiError> {
    let response = container
        .initialize_database_use_case()
        .execute()
        .await
        .map_err(ApiError::from)?;

    let schema_version = response.schema_version.unwrap_or(0);
    let message = if response.is_new_database {
        format!("Database initialized (schema v{})", schema_version)
    } else {
        format!("Database ready (schema v{})", schema_version)
    };

    Ok(message)
}
