//! File plugin.
//!
//! Provides file indexing, content, and metadata commands.

pub mod commands;

use crate::features::file::commands as file_commands;
use crate::interfaces::commands::document_list;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Emitter, Manager, Runtime,
};

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("file")
        .setup(|app, _api| {
            let container = app.state::<crate::interfaces::di::Container>();
            let indexing_state = container.indexing.indexing_state().clone();
            let app_handle = app.clone();

            tauri::async_runtime::spawn(async move {
                let mut rx = indexing_state.subscribe();
                while let Ok(progress) = rx.recv().await {
                    if let Err(e) = app_handle.emit("indexing://progress", progress) {
                        tracing::error!("Failed to emit indexing progress: {}", e);
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::index_file,
            commands::index_directory,
            file_commands::get_file_metadata,
            file_commands::read_file_content,
            file_commands::read_file_bytes,
            commands::get_file_content,
            commands::update_file_metadata,
            commands::delete_file_index,
            commands::remove_indexed_file,
            commands::list_indexed_files,
            commands::list_all_documents,
            commands::get_indexing_status,
            commands::get_indexing_stats,
            commands::get_index_progress,
            commands::cancel_indexing,
            commands::pause_indexing,
            commands::resume_indexing,
            commands::clear_indexing_failure,
            commands::reindex_file,
            commands::delete_document,
            commands::rename_document,
            commands::validate_file_path,
            file_commands::open_file,
            file_commands::open_file_by_id,
            commands::get_file_path_by_id,
            file_commands::show_in_folder,
            commands::get_indexed_folders,
            file_commands::remove_indexed_folder,
            commands::get_indexing_activities,
            commands::get_recent_documents,
            document_list::get_document,
            commands::get_corpus_shape,
            commands::list_conversations_citing_document,
        ])
        .build()
}
