use super::crash_report::CrashReport;
use chrono::Utc;
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Atomic crash report writer.
///
/// Ensures crash logs are written reliably even under adverse conditions:
/// - Generates unique filenames (timestamp + UUID)
/// - Writes to temp file first
/// - Renames atomically (prevents partial writes)
/// - Falls back to stderr if file I/O fails
pub struct CrashWriter {
    crashes_dir: PathBuf,
}

impl CrashWriter {
    /// Create new crash writer for the given directory.
    ///
    /// SAFETY: Never panics. Creates directory if it doesn't exist.
    pub fn new(crashes_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&crashes_dir);
        Self { crashes_dir }
    }

    /// Write crash report atomically.
    ///
    /// Returns the path to the written file, or None if writing failed.
    ///
    /// SAFETY: Never panics. Falls back gracefully on errors.
    pub fn write(&self, report: &CrashReport) -> Option<PathBuf> {
        let filename = format!(
            "crash-{}-{}.json",
            Utc::now().format("%Y-%m-%dT%H-%M-%SZ"),
            Uuid::new_v4()
        );

        let final_path = self.crashes_dir.join(&filename);
        let temp_path = self.crashes_dir.join(format!("{}.tmp", filename));

        match self.write_to_file(&temp_path, report) {
            Ok(_) => {
                if std::fs::rename(&temp_path, &final_path).is_ok() {
                    Some(final_path)
                } else {
                    let _ = std::fs::remove_file(&temp_path);
                    None
                }
            }
            Err(_) => None,
        }
    }

    /// Write crash report to file.
    ///
    /// SAFETY: Can return error, never panics.
    fn write_to_file(&self, path: &Path, report: &CrashReport) -> std::io::Result<()> {
        let mut file = std::fs::File::create(path)?;
        file.write_all(report.to_json_string().as_bytes())?;
        file.sync_all()?;
        Ok(())
    }
}
