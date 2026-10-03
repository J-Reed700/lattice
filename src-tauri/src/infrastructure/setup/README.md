# Setup Module

Startup and shutdown for the Tauri app. `src-tauri/src/main.rs` calls into this
module; everything else about the app is built by the DI container it creates.

## Module structure

```
setup/
├── mod.rs                       # Re-exports + show_error_dialog
├── app.rs                       # initialize_app: the startup sequence
├── directories.rs               # setup_app_directories, setup_model_directory
├── database.rs                  # setup_database: open lattice.db + run migrations
├── embedding.rs                 # setup_tokenizer (optional legacy tokenizer.json)
├── observability.rs             # setup_tracing (see infrastructure/observability)
├── background_workers.rs        # Owns worker handles so shutdown can wait on them
├── degraded_mocks.rs            # Stand-in services when a dependency fails to start
├── renderer_shutdown.rs         # Lets the renderer finish writes before quit
├── renderer_shutdown_macos.rs   # Routes AppKit Quit through the same save gate
├── shutdown.rs                  # graceful_shutdown
└── tests.rs
```

## Startup order (`main.rs`)

1. `tauri_plugin_single_instance`: a second launch focuses the running window
   instead of starting another backend.
2. `setup_tracing()`: stdout + daily log file, optional OpenTelemetry export.
3. `renderer_shutdown::install(app)`.
4. `initialize_app(app)` (`app.rs`), which blocks on `initialize_app_async`:
   - `setup_app_directories`: creates the Tauri app data dir
     (`~/Library/Application Support/tech.lattice.app/` on macOS);
   - `setup_model_directory`: creates `<app_data>/models`;
   - `setup_database(<app_data>/lattice.db)`: opens the pool (WAL, foreign
     keys on, corrupt files moved aside) and applies every pending sqlx
     migration from `src-tauri/migrations/`;
   - wires the audit sink and security context, reads `settings.json`, builds
     the DI `Container`, and starts background work (auto-backup scheduler,
     stale-download reconciliation, orphaned session and model-file cleanup).
5. `lattice::plugins::init_plugins()` registers the feature plugins.

If initialization fails, the error is logged, shown with `show_error_dialog`
(blocking), `graceful_shutdown` runs, and the process exits with status 1.
Tauri would otherwise turn a setup-hook `Err` into an abort inside a macOS
callback.

## Shutdown

`graceful_shutdown(app_handle)` runs on exit and on failed startup. It closes
background task admission and cancels workers, kills every `llama-server`
sidecar synchronously (`SidecarRegistry::kill_all`), waits for workers, flushes
the vector index, closes the database (10 s timeout, 15 s total), and flushes
tracing. Before any of that, `renderer_shutdown::defer` holds a window close or
quit until the renderer has finished its pending repository writes;
`renderer_shutdown_macos.rs` routes AppKit's Quit menu and Dock Quit through the
same gate.

## Error messages

Setup functions return `String` errors written for end users: what failed and
where, a "Possible causes" list, a "Suggested actions" list, then the original
error.

## Testing

```bash
cd src-tauri
cargo test --lib infrastructure::setup
```
