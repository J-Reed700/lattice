//! Corpus-shape Tauri plugin.
//!
//! Exposes user-triggered clustering commands to the frontend.

use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

use crate::features::corpus_shape::commands as corpus_shape_commands;

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("corpus-shape")
        .invoke_handler(tauri::generate_handler![
            corpus_shape_commands::cluster_vault_debug,
            corpus_shape_commands::cluster_vault_run,
            corpus_shape_commands::list_clusters,
        ])
        .build()
}
