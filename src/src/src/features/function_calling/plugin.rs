//! Function Calling Plugin - LLM tool discovery and execution
//!
//! Exposes function-calling commands through the plugin IPC surface.

use crate::features::function_calling::commands as function_calling_commands;
use crate::features::function_calling::domain::{FunctionCall, FunctionResult, RegistryStats};
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

#[tauri::command]
#[specta::specta]
pub async fn list_available_functions(
    container: State<'_, Container>,
) -> Result<Vec<super::domain::ToolDefinition>, ApiError> {
    function_calling_commands::list_available_functions(container)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn execute_function(
    call: FunctionCall,
    container: State<'_, Container>,
) -> Result<FunctionResult, ApiError> {
    function_calling_commands::execute_function(container, call)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_function_stats(
    container: State<'_, Container>,
) -> Result<RegistryStats, ApiError> {
    function_calling_commands::get_function_stats(container)
        .await
        .map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("functions")
        .invoke_handler(tauri::generate_handler![
            list_available_functions,
            execute_function,
            get_function_stats,
        ])
        .build()
}
