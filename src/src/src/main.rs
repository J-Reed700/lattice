#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Import from the library crate
use lattice::infrastructure::setup;

// Pure Plugin Architecture - All IPC commands are Tauri plugins.
// Commands follow the Diamond Standard pattern with direct *_impl() calls.

fn run_app() -> Result<(), Box<dyn std::error::Error>> {
    use tauri::Manager;
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Initialize tracing FIRST - now we have Tokio runtime available for OTEL
            setup::setup_tracing();

            // Sprint 6 PR 6.1: register the sidecar registry BEFORE
            // anything else that might spawn a sidecar. The DI
            // Container's LLM factory looks this up via
            // `app.try_state::<SidecarRegistry>()` when starting a
            // llama-server child process. Registering first guarantees
            // every spawned sidecar enrolls for the shutdown sweep.
            // Sprint 6 PR 6.1 also runs an orphan scan here to clean
            // up any sidecars left behind by a previous crash.
            app.manage(lattice::llm::sidecar_manager::SidecarRegistry::new());
            lattice::llm::sidecar_manager::reap_orphan_sidecars();

            // CRITICAL: Initialize app and manage Container FIRST
            // This must happen before plugins try to access the Container
            setup::initialize_app(app)?;

            // NOW initialize domain plugins - they can safely access Container
            // Pure plugin architecture - no gateway, all commands are plugins
            for plugin in lattice::plugins::init_plugins() {
                app.handle().plugin(plugin)?;
            }

            Ok(())
        })
        .build(tauri::generate_context!())?
        .run(|app_handle, event| {
            match &event {
                // Cmd-Q on macOS, system-shutdown on Windows/Linux,
                // or anything that explicitly asks Tauri to exit.
                tauri::RunEvent::ExitRequested { .. } => {
                    setup::graceful_shutdown(app_handle);
                }
                // Sprint 6 PR 6.1: macOS window-close gap. Clicking
                // the red X on macOS does NOT fire `ExitRequested`
                // by default — Tauri keeps the app alive in the
                // dock. Our users expect "close window = quit"
                // (Lattice is not a menu-bar app). When the last
                // window is destroyed, walk through the same
                // shutdown sequence and request Tauri exit.
                #[cfg(target_os = "macos")]
                tauri::RunEvent::WindowEvent {
                    event: tauri::WindowEvent::Destroyed,
                    ..
                } => {
                    use tauri::Manager;
                    if app_handle.webview_windows().is_empty() {
                        tracing::info!("Last window destroyed on macOS; triggering app shutdown");
                        setup::graceful_shutdown(app_handle);
                        app_handle.exit(0);
                    }
                }
                _ => {}
            }
        });

    Ok(())
}

fn main() {
    lattice::infrastructure::crash::install_panic_hook();
    // Note: setup_tracing() moved to .setup() hook where Tokio runtime is available for OTEL

    if let Err(e) = run_app() {
        eprintln!("Fatal error: Failed to run application: {}", e);
        std::process::exit(1);
    }
}
