//! Credentials Plugin - Secure API key storage
//!
//! Migrated from ipc/domains/credentials.rs as part of Operation Scorched Earth

use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

pub mod commands;
pub mod types;

pub use commands::*;
pub use types::*;

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("credentials")
        .invoke_handler(tauri::generate_handler![
            commands::credentials_store,
            commands::credentials_get,
            commands::credentials_delete,
            commands::credentials_has,
            commands::credentials_clear_all,
            commands::credentials_set_endpoint,
            commands::credentials_get_endpoint,
        ])
        .build()
}
