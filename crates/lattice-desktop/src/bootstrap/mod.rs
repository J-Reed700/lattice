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
pub mod async_runtime;
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

#[cfg(test)]
mod tests {
    /// Without the claim the app would resolve a scratch home and show an
    /// empty library, with nothing failing. `main` is not callable from a
    /// test, so its source is what is checked.
    #[test]
    fn the_app_claims_the_users_home_before_it_runs() {
        let main = include_str!("../main.rs");
        let body = main
            .split_once("fn main() {")
            .map(|(_, body)| body)
            .expect("main.rs has a main");
        let claim = body
            .find("roots::use_the_users_home();")
            .expect("main claims the user's home");
        let run = body.find("run_app()").expect("main runs the app");
        assert!(
            claim < run,
            "main claims the user's home after the app runs"
        );
    }
}
