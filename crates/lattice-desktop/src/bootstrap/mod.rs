//! The composition root: the app's startup sequence and its shutdown.
//!
//! Startup wiring has to name every feature it starts, so it lives here
//! rather than in `infrastructure`, which may not import features. Nothing
//! else depends on it: only the crate entry points (`main.rs`, `lib.rs` and
//! `bin/*`) may reference this module, which the architecture ratchet
//! enforces as `composition-root-import`. Plain setup steps with no feature
//! imports (directories, database, tracing, tokenizer, the renderer shutdown
//! hook) stay in `infrastructure::setup` and are called from here.

pub mod app;
mod background_workers;
pub mod shutdown;
mod startup_reconciliation;

pub use app::initialize_app;
pub use shutdown::{graceful_shutdown, SidecarSweep};

pub fn show_error_dialog(app: &tauri::AppHandle, title: &str, message: &str) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
    app.dialog()
        .message(message)
        .title(title)
        .kind(MessageDialogKind::Error)
        .blocking_show();
}
