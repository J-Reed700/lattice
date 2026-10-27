//! Context for errors on their way up: [`ResultExt`] wraps any error that
//! converts into an [`AppError`], or a missing value, with what was being done.

use super::{AppError, Result};

/// Extension trait for adding context to Result types.
///
/// This trait provides convenient methods to add contextual information
/// to errors, making debugging and error messages more helpful.
pub trait ResultExt<T> {
    /// Add static context to an error.
    fn context(self, msg: impl Into<String>) -> Result<T>;

    /// Add dynamic context to an error using a closure.
    /// The closure is only called if there's an error.
    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String;
}

impl<T, E> ResultExt<T> for std::result::Result<T, E>
where
    E: Into<AppError>,
{
    fn context(self, msg: impl Into<String>) -> Result<T> {
        self.map_err(|e| {
            let base_error = e.into();
            AppError::Other(format!("{}: {}", msg.into(), base_error))
        })
    }

    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| {
            let base_error = e.into();
            AppError::Other(format!("{}: {}", f(), base_error))
        })
    }
}

impl<T> ResultExt<T> for Option<T> {
    fn context(self, msg: impl Into<String>) -> Result<T> {
        self.ok_or_else(|| AppError::Other(msg.into()))
    }

    fn with_context<F>(self, f: F) -> Result<T>
    where
        F: FnOnce() -> String,
    {
        self.ok_or_else(|| AppError::Other(f()))
    }
}
