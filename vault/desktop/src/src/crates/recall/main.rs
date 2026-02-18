#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Import from the library crate
use vault::infrastructure::setup;

/// Pure Plugin Architecture - All IPC commands are Tauri plugins
/// Operation Scorched Earth Complete: Gateway eliminated, 16 plugins provide 59 commands
/// All commands follow Diamond Standard pattern with direct *_impl() calls

fn run_app() -> Result<(), Box<dyn std::error::Error>> {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Initialize tracing FIRST - now we have Tokio runtime available for OTEL
            setup::setup_tracing();

            // CRITICAL: Initialize app and manage Container FIRST
            // This must happen before plugins try to access the Container
            setup::initialize_app(app)?;

            // NOW initialize domain plugins - they can safely access Container
            // Pure plugin architecture - no gateway, all commands are plugins
            for plugin in vault::plugins::init_plugins() {
                app.handle().plugin(plugin)?;
            }

            Ok(())
        })
        .build(tauri::generate_context!())?
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                setup::graceful_shutdown(app_handle);
            }
        });

    Ok(())
}

fn main() {
    vault::infrastructure::crash::install_panic_hook();
    // Note: setup_tracing() moved to .setup() hook where Tokio runtime is available for OTEL

    if let Err(e) = run_app() {
        eprintln!("Fatal error: Failed to run application: {}", e);
        std::process::exit(1);
    }
}
