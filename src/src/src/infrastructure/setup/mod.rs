pub mod app;
mod background_workers;
pub mod database;
pub mod degraded_mocks;
pub mod directories;
pub mod embedding;
pub mod observability;
pub mod renderer_shutdown;
pub mod shutdown;

#[cfg(test)]
mod tests;

pub use app::initialize_app;
pub use database::setup_database;
pub use directories::{setup_app_directories, setup_model_directory};
pub use embedding::{setup_embedding_service, setup_tokenizer};
pub use observability::setup_tracing;
pub use shutdown::graceful_shutdown;

pub fn show_error_dialog(app: &tauri::AppHandle, title: &str, message: &str) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
    app.dialog()
        .message(message)
        .title(title)
        .kind(MessageDialogKind::Error)
        .blocking_show();
}
