//! Health Plugin - System health and diagnostics
//!
//! Migrated from ipc/domains/health.rs as part of Operation Scorched Earth

use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

pub mod commands;
pub mod types;

pub use commands::*;
pub use types::*;

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("health")
        .invoke_handler(tauri::generate_handler![
            commands::health_check,
            commands::get_system_stats,
            commands::get_version,
            commands::initialize_database,
        ])
        .build()
}
