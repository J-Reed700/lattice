#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Import from the library crate
use lattice::infrastructure::setup;

#[cfg(feature = "desktop-e2e")]
mod desktop_e2e;

// IPC commands are exposed through domain-specific Tauri plugins.

fn run_app() -> Result<(), Box<dyn std::error::Error>> {
    use tauri::Manager;
    let builder = tauri::Builder::default();
    #[cfg(feature = "desktop-e2e")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    builder
        // First, so a second launch hands over to the running app before any
        // of its own setup runs: that setup would reap the first instance's
        // llama-server mid-turn and sweep and migrate the same database.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Tracing requires the Tokio runtime available inside this hook.
            setup::setup_tracing();
            if let Ok(resources) = app.path().resource_dir() {
                if let Err(error) =
                    lattice::features::learning::python_runtime::configure_python_runtime(
                        resources.join("resources/learning-python"),
                    )
                {
                    tracing::warn!(%error, "Bundled Python runtime could not be configured");
                }
            }

            // Register before the webview can announce readiness. On macOS
            // this also routes AppKit's Cmd-Q/Dock Quit through the save gate.
            setup::renderer_shutdown::install(app.handle())?;

            // Lets a page that refuses the HTTP client be read in a hidden
            // window of the app's own browser engine.
            lattice::features::web::services::browser_reader::install(app.handle().clone());

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
                        setup::renderer_shutdown::finish_native_termination(app_handle);
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
                    let user_windows_left = app_handle.webview_windows().keys().any(|label| {
                        !lattice::features::web::services::browser_reader::is_reader_window(label)
                    });
                    if !user_windows_left {
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
    // A learner's lab code runs in a child copy of this executable. It serves
    // that one run and exits before any app state, window or plugin exists.
    if lattice::features::learning::lab_runner::is_runner_invocation() {
        std::process::exit(lattice::features::learning::lab_runner::run_child());
    }
    lattice::infrastructure::crash::install_panic_hook();
    // This entrypoint exists only in an instrumented, isolated test package.
    // Exercise the installed executable's signing policy and bundled assets
    // without adding a production IPC command or requiring a model response.
    #[cfg(feature = "desktop-e2e")]
    {
        let mut args = std::env::args_os().skip(1);
        if args.next().as_deref() == Some(std::ffi::OsStr::new("--learning-runtime-self-test")) {
            let Some(resources) = args.next() else {
                eprintln!("A bundled Python resource directory is required.");
                std::process::exit(2);
            };
            if let Err(error) = desktop_e2e::learning_runtime_self_test::run(resources.into()) {
                eprintln!("Embedded runtime self-test failed: {error}");
                std::process::exit(1);
            }
            return;
        }
    }
    // Note: setup_tracing() moved to .setup() hook where Tokio runtime is available for OTEL

    if let Err(e) = run_app() {
        eprintln!("Fatal error: Failed to run application: {}", e);
        std::process::exit(1);
    }
}
