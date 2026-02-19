//! Initialization commands for first-time setup
//!
//! These commands handle downloading models and initializing the database
//! on first run or when the user needs to reset their installation.

use tauri::{AppHandle, Manager};

/// Initializes embedding models for semantic search
///
/// **Note**: This command is a legacy compatibility stub. Models are now automatically
/// initialized at application startup in `app.rs`. This command exists only for frontend
/// compatibility and immediately returns success.
///
/// # Arguments
///
/// * `_app_handle` - Application handle (unused, models initialized at startup)
///
/// # Returns
///
/// * `Ok(String)` - Success message indicating models are initialized
///
/// # Errors
///
/// This command does not currently return errors as initialization happens at startup.
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// // Initialize models (legacy compatibility)
/// const result = await invoke<string>('initialize_models');
/// console.log(result); // "Models initialized successfully"
/// ```
///
/// # Migration Note
///
/// This command is **deprecated** and exists only for backward compatibility. Model
/// initialization now occurs automatically during application startup via the model
/// loading system in `app.rs`. Frontend code should be updated to remove calls to
/// this command.
///
/// # Command Flow
///
/// 1. Returns success immediately (models already loaded at startup)
pub async fn initialize_models(_app_handle: AppHandle) -> Result<String, String> {
    // Models are already loaded at startup in app.rs
    // This command is just for frontend compatibility
    Ok("Models initialized successfully".to_string())
}

/// Initializes the application database with schema migrations
///
/// **Note**: Database is already initialized at application startup. This command
/// exists for legacy frontend compatibility and returns success immediately.
pub async fn initialize_database(_app_handle: AppHandle) -> Result<String, String> {
    // Database is already initialized at startup in app.rs
    // This command is just for frontend compatibility
    Ok("Database already initialized at startup".to_string())
}
