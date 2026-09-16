//! Search Plugin
//!
//! Registers search commands so the frontend can invoke them via plugin:search|command.
//! This plugin registers search commands from interfaces/commands.

pub mod commands;

use crate::features::search::commands as search_commands;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("search")
        .invoke_handler(tauri::generate_handler![
            search_commands::search_documents,
            search_commands::search_fast,
            search_commands::semantic_search,
            search_commands::hybrid_search,
            search_commands::find_similar,
            search_commands::find_similar_documents,
            search_commands::search_with_recency,
            search_commands::batch_search,
        ])
        .build()
}
