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
use std::sync::OnceLock;

static CRASH_WRITER: OnceLock<CrashWriter> = OnceLock::new();
static CRASHES_DIR: OnceLock<PathBuf> = OnceLock::new();

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
    let crashes_dir = get_crashes_directory();
    let writer = CrashWriter::new(crashes_dir.clone());

    let _ = CRASH_WRITER.set(writer);
    let _ = CRASHES_DIR.set(crashes_dir);

    std::panic::set_hook(Box::new(move |panic_info| {
        let report = CrashReport::from_panic(panic_info);

        eprintln!("\n=== APPLICATION CRASH ===");
        eprintln!("{}", report.to_json_string());
        eprintln!("=========================\n");

        if let Some(writer) = CRASH_WRITER.get() {
            if let Some(path) = writer.write(&report) {
                eprintln!("Crash report saved to: {}", path.display());

                if let Some(crashes_dir) = CRASHES_DIR.get() {
                    CrashRotation::cleanup_old_crashes(crashes_dir);
                }
            } else {
                eprintln!("WARNING: Failed to write crash report to file");
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

    let _ = std::fs::create_dir_all(&crashes_dir);

    let writer = CrashWriter::new(crashes_dir.clone());
    let _ = CRASH_WRITER.set(writer);
    let _ = CRASHES_DIR.set(crashes_dir);
}

/// Get the crashes directory path.
///
/// Tries multiple fallback locations:
/// 1. {app_data_dir}/crashes (ideal)
/// 2. {current_dir}/crashes (fallback if app_data_dir unavailable)
/// 3. {temp_dir}/recall-crashes (last resort)
fn get_crashes_directory() -> PathBuf {
    std::env::current_dir()
        .ok()
        .map(|d| d.join("crashes"))
        .unwrap_or_else(|| std::env::temp_dir().join("recall-crashes"))
}
