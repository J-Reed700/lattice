//! Config Plugin - App configuration management

use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

pub mod commands;
pub mod types;

pub use commands::*;
pub use types::*;

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("config")
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::get_watch_folders,
            commands::add_watch_folder,
            commands::remove_watch_folder,
        ])
        .build()
}
