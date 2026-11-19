//! File plugin.
//!
//! Provides file indexing, content, and metadata commands.

pub mod commands;

use crate::features::file::commands as file_commands;
use crate::interfaces::commands::document_list;
use crate::interfaces::commands::indexing;
use tauri::{
    plugin::{Builder, TauriPlugin},
    Emitter, Manager, Runtime,
};

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("file")
        .setup(|app, _api| {
            let container = app.state::<crate::interfaces::di::Container>();
            let indexing_state = container.indexing.indexing_state().clone();
            let shutdown_token = app
                .try_state::<tokio_util::sync::CancellationToken>()
                .map(|token| token.inner().clone())
                .unwrap_or_else(|| container.core.background().token());
            let app_handle = app.clone();
            let mut rx = indexing_state.subscribe();

            // raw-spawn: plugin setup has no tokio context; ends on shutdown or when indexing state drops
            tauri::async_runtime::spawn(async move {
                loop {
                    let received = tokio::select! {
                        biased;
                        () = shutdown_token.cancelled() => break,
                        received = rx.recv() => received,
                    };
                    match received {
                        Ok(progress) => {
                            if let Err(e) = app_handle.emit("indexing://progress", progress) {
                                tracing::error!("Failed to emit indexing progress: {}", e);
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                skipped,
                                "Indexing progress listener lagged; emitting current snapshot"
                            );
                            let snapshot = indexing_state.get_snapshot();
                            if let Err(e) = app_handle.emit("indexing://progress", snapshot) {
                                tracing::error!(
                                    "Failed to emit indexing progress snapshot: {}",
                                    e
                                );
                            }
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            indexing::index_file,
            indexing::index_directory,
            file_commands::get_file_metadata,
            file_commands::read_file_content,
            file_commands::read_file_bytes,
            indexing::remove_indexed_file,
            commands::list_indexed_files,
            commands::list_all_documents,
            indexing::get_indexing_status,
            indexing::get_indexing_stats,
            indexing::get_index_progress,
            indexing::cancel_indexing,
            indexing::pause_indexing,
            indexing::resume_indexing,
            indexing::clear_indexing_failure,
            indexing::reindex_file,
            indexing::delete_document,
            indexing::rename_document,
            file_commands::open_file,
            file_commands::open_file_by_id,
            commands::get_file_path_by_id,
            file_commands::show_in_folder,
            commands::get_indexed_folders,
            commands::get_indexing_activities,
            commands::get_recent_documents,
            document_list::get_document,
            commands::get_corpus_shape,
            commands::list_conversations_citing_document,
            crate::features::file::custom_collections::commands::list_custom_collections,
            crate::features::file::custom_collections::commands::create_custom_collection,
            crate::features::file::custom_collections::commands::rename_custom_collection,
            crate::features::file::custom_collections::commands::move_custom_collection,
            crate::features::file::custom_collections::commands::delete_custom_collection,
            crate::features::file::custom_collections::commands::add_documents_to_custom_collection,
            crate::features::file::custom_collections::commands::remove_documents_from_custom_collection,
        ])
        .build()
}
