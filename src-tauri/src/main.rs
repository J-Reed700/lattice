#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Import from the library crate
use lattice::infrastructure::setup;

// IPC commands are exposed through domain-specific Tauri plugins.

fn run_app() -> Result<(), Box<dyn std::error::Error>> {
    use tauri::Manager;
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Tracing requires the Tokio runtime available inside this hook.
            setup::setup_tracing();

            // Register the sidecar registry before anything can spawn a
            // sidecar. The LLM factory looks this up via
            // `app.try_state::<SidecarRegistry>()` when starting a
            // llama-server child process. Registering first guarantees
            // every spawned sidecar is included in the shutdown sweep. The
            // orphan scan cleans up sidecars left by a previous crash.
            app.manage(lattice::features::llm::engine::sidecar_manager::SidecarRegistry::new());
            lattice::features::llm::engine::sidecar_manager::reap_orphan_sidecars();

            // Plugins require the managed container. A failed initialization
            // shows its dialog and exits the process from inside this call;
            // an `Err` returned from this hook would abort the app instead.
            setup::initialize_app(app);

            // The container is now available to each domain plugin.
            for plugin in lattice::plugins::init_plugins() {
                app.handle().plugin(plugin)?;
            }

            setup::renderer_shutdown::install(app.handle());

            Ok(())
        })
        .build(tauri::generate_context!())?
        .run(|app_handle, event| {
            match &event {
                // Cmd-Q on macOS, system-shutdown on Windows/Linux,
                // or anything that explicitly asks Tauri to exit.
                tauri::RunEvent::ExitRequested { api, .. } => {
                    if setup::renderer_shutdown::defer(app_handle) {
                        api.prevent_exit();
                    } else {
                        setup::graceful_shutdown(app_handle);
                    }
                }
                tauri::RunEvent::WindowEvent {
                    label,
                    event: tauri::WindowEvent::CloseRequested { api, .. },
                    ..
                } if label == "main" && setup::renderer_shutdown::defer(app_handle) => {
                    api.prevent_close();
                }
                // Clicking the red close button on macOS does not fire
                // `ExitRequested`
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
