pub mod crash_metadata;
pub mod crash_report;
pub mod crash_rotation;
pub mod crash_writer;

#[cfg(test)]
mod tests;

use crash_report::CrashReport;
use crash_rotation::CrashRotation;
use crash_writer::CrashWriter;
use std::path::PathBuf;
use std::sync::RwLock;

/// Where crash reports go. Set once at the top of `main`, before the app-data
/// directory is known, then moved there by `set_crashes_directory`. A lock
/// rather than a `OnceLock` so that the move takes effect.
static CRASHES_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

fn store_crashes_directory(dir: PathBuf) {
    let mut slot = CRASHES_DIR
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *slot = Some(dir);
}

fn crashes_directory() -> Option<PathBuf> {
    CRASHES_DIR
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// Install the global panic hook.
///
/// This function MUST be called early in main(), before any other initialization.
/// It captures the app data directory for crash log storage.
///
/// # Example
///
/// ```rust
/// install_panic_hook();
/// // ... rest of app initialization
/// ```
///
/// # Crash Log Location
///
/// Crashes are logged to: `{app_data_dir}/crashes/crash-{timestamp}-{uuid}.json`
///
/// # Safety
///
/// This function never panics. If initialization fails, crash reports are
/// written to stderr as a fallback.
pub fn install_panic_hook() {
    store_crashes_directory(get_crashes_directory());

    std::panic::set_hook(Box::new(move |panic_info| {
        let report = CrashReport::from_panic(panic_info);
        let json = report.to_json_string();

        eprintln!("\n=== APPLICATION CRASH ===");
        eprintln!("{json}");
        eprintln!("=========================\n");
        // A packaged app has no visible stderr; the log file is what a user
        // can send.
        tracing::error!(report = %json, "Application panic");

        let Some(crashes_dir) = crashes_directory() else {
            return;
        };
        match CrashWriter::new(crashes_dir.clone()).write(&report) {
            Some(path) => {
                eprintln!("Crash report saved to: {}", path.display());
                tracing::error!(path = %path.display(), "Crash report saved");
                CrashRotation::cleanup_old_crashes(&crashes_dir);
            }
            None => {
                eprintln!("WARNING: Failed to write crash report to file");
                tracing::error!(
                    dir = %crashes_dir.display(),
                    "Failed to write crash report to file"
                );
            }
        }
    }));
}

/// Set the crashes directory to app data directory.
///
/// This should be called in setup::initialize_app() once the app handle is available.
///
/// # Example
///
/// ```rust
/// pub fn initialize_app(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
///     let app_dir = app.path().app_data_dir()?;
///     crate::infrastructure::crash::set_crashes_directory(app_dir);
///     // ... rest of initialization
/// }
/// ```
pub fn set_crashes_directory(app_data_dir: PathBuf) {
    let crashes_dir = app_data_dir.join("crashes");

    if let Err(e) = std::fs::create_dir_all(&crashes_dir) {
        tracing::warn!(
            dir = %crashes_dir.display(),
            error = %e,
            "Could not create the crash report directory"
        );
    }
    store_crashes_directory(crashes_dir);
}

/// Get the crashes directory path.
///
/// Tries multiple fallback locations:
/// 1. {app_data_dir}/crashes (ideal)
/// 2. {current_dir}/crashes (fallback if app_data_dir unavailable)
/// 3. {temp_dir}/lattice-crashes (last resort)
fn get_crashes_directory() -> PathBuf {
    std::env::current_dir()
        .ok()
        .map(|d| d.join("crashes"))
        .unwrap_or_else(|| std::env::temp_dir().join("lattice-crashes"))
}
