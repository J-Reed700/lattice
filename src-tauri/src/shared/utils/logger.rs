use chrono::Local;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Logger {
    log_dir: PathBuf,
    current_file: Mutex<Option<File>>,
    max_file_size: u64,
    max_files: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl LogLevel {
    fn as_str(&self) -> &str {
        match self {
            LogLevel::Error => "ERROR",
            LogLevel::Warn => "WARN",
            LogLevel::Info => "INFO",
            LogLevel::Debug => "DEBUG",
        }
    }
}

impl Logger {
    pub fn new(log_dir: PathBuf) -> std::io::Result<Self> {
        fs::create_dir_all(&log_dir)?;

        Ok(Self {
            log_dir,
            current_file: Mutex::new(None),
            max_file_size: 10 * 1024 * 1024,
            max_files: 5,
        })
    }

    pub fn log(&self, level: LogLevel, message: &str, context: Option<&str>) {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let log_entry = if let Some(ctx) = context {
            format!(
                "[{}] [{}] [{}] {}\n",
                timestamp,
                level.as_str(),
                ctx,
                message
            )
        } else {
            format!("[{}] [{}] {}\n", timestamp, level.as_str(), message)
        };

        if let Err(e) = self.write_log(&log_entry) {
            eprintln!("Failed to write log: {}", e);
        }
    }

    pub fn error(&self, message: &str, context: Option<&str>) {
        self.log(LogLevel::Error, message, context);
    }

    pub fn warn(&self, message: &str, context: Option<&str>) {
        self.log(LogLevel::Warn, message, context);
    }

    pub fn info(&self, message: &str, context: Option<&str>) {
        self.log(LogLevel::Info, message, context);
    }

    pub fn debug(&self, message: &str, context: Option<&str>) {
        self.log(LogLevel::Debug, message, context);
    }

    fn write_log(&self, entry: &str) -> std::io::Result<()> {
        let mut file_guard = self.current_file.lock().map_err(|e| {
            std::io::Error::other(format!(
                "Failed to acquire lock on log file: {}. Logger may be in inconsistent state.",
                e
            ))
        })?;

        let current_log_path = self.get_current_log_path();

        if file_guard.is_none() {
            *file_guard = Some(self.open_log_file(&current_log_path)?);
        }

        if let Some(ref mut _file) = *file_guard {
            if self.should_rotate(&current_log_path)? {
                drop(file_guard);
                self.rotate_logs()?;
                file_guard = self.current_file.lock().map_err(|e| {
                    std::io::Error::other(format!(
                        "Failed to reacquire lock after log rotation: {}",
                        e
                    ))
                })?;
                *file_guard = Some(self.open_log_file(&current_log_path)?);
            }

            if let Some(ref mut file) = *file_guard {
                file.write_all(entry.as_bytes())?;
                file.flush()?;
            }
        }

        Ok(())
    }

    fn get_current_log_path(&self) -> PathBuf {
        let date = Local::now().format("%Y-%m-%d");
        self.log_dir.join(format!("lattice-{}.log", date))
    }

    fn open_log_file(&self, path: &PathBuf) -> std::io::Result<File> {
        OpenOptions::new().create(true).append(true).open(path)
    }

    fn should_rotate(&self, path: &PathBuf) -> std::io::Result<bool> {
        if let Ok(metadata) = fs::metadata(path) {
            Ok(metadata.len() > self.max_file_size)
        } else {
            Ok(false)
        }
    }

    fn rotate_logs(&self) -> std::io::Result<()> {
        let mut log_files: Vec<_> = fs::read_dir(&self.log_dir)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext == "log")
                    .unwrap_or(false)
            })
            .collect();

        log_files.sort_by_key(|entry| entry.metadata().and_then(|m| m.modified()).ok());

        while log_files.len() >= self.max_files {
            if let Some(oldest) = log_files.first() {
                fs::remove_file(oldest.path())?;
                log_files.remove(0);
            } else {
                break;
            }
        }

        let current_path = self.get_current_log_path();
        if current_path.exists() {
            let timestamp = Local::now().format("%Y-%m-%d-%H%M%S");
            let rotated_path = self.log_dir.join(format!("lattice-{}.log", timestamp));
            fs::rename(&current_path, &rotated_path)?;
        }

        Ok(())
    }

    pub fn log_error_with_context(
        &self,
        error: &crate::shared::error::AppError,
        operation: &str,
        file_path: Option<&str>,
    ) {
        let context = if let Some(path) = file_path {
            format!("{}::{}", operation, path)
        } else {
            operation.to_string()
        };

        let message = format!(
            "{}\nUser message: {}\nRecoverable: {}, User fixable: {}, Fatal: {}",
            error,
            error.to_user_friendly_message(),
            error.is_recoverable(),
            error.is_user_fixable(),
            error.is_fatal()
        );

        self.error(&message, Some(&context));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_logger_creation() {
        let dir = tempdir().unwrap();
        let logger = Logger::new(dir.path().to_path_buf()).unwrap();

        logger.info("Test message", None);

        let log_files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();

        assert_eq!(log_files.len(), 1);
    }

    #[test]
    fn test_log_levels() {
        let dir = tempdir().unwrap();
        let logger = Logger::new(dir.path().to_path_buf()).unwrap();

        logger.error("Error message", Some("test_context"));
        logger.warn("Warning message", None);
        logger.info("Info message", None);
        logger.debug("Debug message", None);
    }
}
