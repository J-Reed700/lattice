use std::ffi::OsStr;
use std::path::{Path, PathBuf};

const MAX_CRASH_REPORTS: usize = 10;

/// Crash log rotation manager.
///
/// Keeps only the N most recent crash reports to prevent unbounded disk usage.
pub struct CrashRotation;

impl CrashRotation {
    /// Clean up old crash reports, keeping only the N most recent.
    ///
    /// SAFETY: Never panics. Silently ignores I/O errors.
    pub fn cleanup_old_crashes(crashes_dir: &Path) {
        let mut crash_files = match Self::list_crash_files(crashes_dir) {
            Ok(files) => files,
            Err(_) => return,
        };

        if crash_files.len() <= MAX_CRASH_REPORTS {
            return;
        }

        crash_files.sort_by_key(|path| std::fs::metadata(path).and_then(|m| m.modified()).ok());

        let num_to_delete = crash_files.len() - MAX_CRASH_REPORTS;
        for path in crash_files.iter().take(num_to_delete) {
            let _ = std::fs::remove_file(path);
        }
    }

    /// List all crash report files in directory.
    ///
    /// SAFETY: Returns error if directory read fails, never panics.
    fn list_crash_files(crashes_dir: &Path) -> std::io::Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        for entry in std::fs::read_dir(crashes_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension() == Some(OsStr::new("json")) {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("crash-") {
                        files.push(path);
                    }
                }
            }
        }

        Ok(files)
    }
}
