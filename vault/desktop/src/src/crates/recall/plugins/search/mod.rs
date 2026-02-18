//! Search Plugin
//!
//! Registers search commands so the frontend can invoke them via plugin:search|command.
//! Uses __cmd__* macros from interfaces/commands/search_commands (same pattern as cache_plugin).

pub mod commands;

use crate::interfaces::commands::search_commands::{
    __cmd__batch_search, __cmd__find_similar, __cmd__hybrid_search, __cmd__search_documents,
    __cmd__search_fast, __cmd__search_with_recency, __cmd__semantic_search, batch_search,
    find_similar, hybrid_search, search_documents, search_fast, search_with_recency,
    semantic_search,
};
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime,
};

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("search")
        .invoke_handler(tauri::generate_handler![
            search_documents,
            search_fast,
            semantic_search,
            hybrid_search,
            find_similar,
            search_with_recency,
            batch_search,
        ])
        .build()
}
