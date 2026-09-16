//! Error logging extension for indexing operations.
//!
//! This module provides a trait-based approach to eliminate repetitive error logging
//! patterns in the indexing pipeline. Instead of manually writing `.map_err(|e| { tracing::error!(...); ... })`
//! throughout the code, you can use `.log_context(...)` for cleaner error handling.
//!
//! # Design
//!
//! The `IndexingResultExt` trait extends `Result` types with a `log_context` method that:
//! - Logs the error with structured fields (context, path, error message)
//! - Converts the error to `IndexingError` (via `Into`)
//! - Returns the error for propagation
//!
//! # Examples
//!
//! ```rust
//! use crate::features::indexing::engine::error_ext::IndexingResultExt;
//! use std::path::Path;
//!
//! async fn index_file(path: &Path) -> Result<(), IndexingError> {
//!     // Before: Repetitive error logging
//!     let content = read_file(path).await.map_err(|e| {
//!         tracing::error!("Failed to read file {}: {}", path.display(), e);
//!         IndexingError::from(e)
//!     })?;
//!
//!     // After: Clean and DRY
//!     let content = read_file(path).await
//!         .log_context("read file", path)?;
//!
//!     Ok(())
//! }
//! ```
//!
//! # Benefits
//!
//! - **DRY**: Eliminates 7+ instances of repetitive error logging code
//! - **Consistent**: All errors logged with the same structured format
//! - **Maintainable**: Change logging format in one place
//! - **Type-safe**: Generic over error types that implement `Display + Into<IndexingError>`

use crate::features::indexing::engine::error::IndexingError;
use std::fmt::Display;
use std::path::Path;

/// Extension trait for adding contextual error logging to `Result` types.
///
/// This trait provides a single method `log_context` that logs errors before
/// propagating them, eliminating the need for repetitive `.map_err(|e| { tracing::error!(...); ... })`
/// patterns throughout the indexing code.
///
/// # Type Parameters
///
/// - `T`: The success type of the Result
///
/// # Generic Constraints
///
/// The trait is implemented for `Result<T, E>` where:
/// - `E: Display` - Error can be formatted for logging
/// - `E: Into<IndexingError>` - Error can be converted to IndexingError
///
/// # Examples
///
/// ```rust
/// use crate::features::indexing::engine::error_ext::IndexingResultExt;
/// use std::path::Path;
///
/// // In an async function
/// let file_record = file_storage
///     .store_file(&path, &mime_type, None)
///     .await
///     .log_context("store file", &path)?;
///
/// let extracted = extractor
///     .extract_from_file(&path)
///     .await
///     .log_context("extract text", &path)?;
///
/// let embeddings = embedder
///     .embed_contextualized_chunks(&chunks)
///     .await
///     .log_context("generate embeddings", &path)?;
/// ```
pub trait IndexingResultExt<T> {
    /// Logs an error with context and path information, then converts to IndexingError.
    ///
    /// This method logs the error using `tracing::error!` with structured fields:
    /// - `context`: Description of the operation that failed (e.g., "store file", "extract text")
    /// - `path`: File path where the operation failed
    /// - `error`: The error message from the underlying error
    ///
    /// After logging, it converts the error to `IndexingError` using the `Into` trait
    /// and returns it, allowing `?` to propagate the error up the call stack.
    ///
    /// # Arguments
    ///
    /// * `context` - A static string describing the operation (e.g., "store file", "extract text")
    /// * `path` - The file path where the operation failed
    ///
    /// # Returns
    ///
    /// - `Ok(T)` if the Result was successful (no logging occurs)
    /// - `Err(IndexingError)` if the Result was an error (after logging)
    ///
    /// # Examples
    ///
    /// ```rust
    /// // Replace this:
    /// let file_storage_result = self.file_storage.store_file(&path, &content)
    ///     .await
    ///     .map_err(|e| {
    ///         tracing::error!("Failed to store file {}: {}", path.display(), e);
    ///         IndexingError::from(e)
    ///     })?;
    ///
    /// // With this:
    /// let file_storage_result = self.file_storage.store_file(&path, &content)
    ///     .await
    ///     .log_context("store file", &path)?;
    /// ```
    fn log_context(self, context: &str, path: &Path) -> Result<T, IndexingError>;
}

impl<T, E> IndexingResultExt<T> for Result<T, E>
where
    E: Display + Into<IndexingError>,
{
    fn log_context(self, context: &str, path: &Path) -> Result<T, IndexingError> {
        self.map_err(|e| {
            let err: IndexingError = e.into();
            tracing::error!(
                context = context,
                path = %path.display(),
                error = %err,
                "Indexing operation failed"
            );
            err
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::error::AppError;
    use std::path::PathBuf;

    #[test]
    fn test_log_context_on_ok() {
        let path = PathBuf::from("/test/file.txt");
        let result: Result<i32, AppError> = Ok(42);

        let logged = result.log_context("test operation", &path);
        assert_eq!(logged.unwrap(), 42);
    }

    #[test]
    fn test_log_context_on_err() {
        let path = PathBuf::from("/test/file.txt");
        let result: Result<i32, AppError> = Err(AppError::NotFound("test error".to_string()));

        let logged = result.log_context("test operation", &path);
        assert!(logged.is_err());
    }

    #[test]
    fn test_log_context_preserves_error_type() {
        let path = PathBuf::from("/test/file.txt");
        let result: Result<String, AppError> = Err(AppError::FileNotFound {
            path: "/test/file.txt".to_string(),
        });

        let logged = result.log_context("read file", &path);
        assert!(logged.is_err());
        let err = logged.unwrap_err();
        // Error should be converted to IndexingError (which is AppError)
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_log_context_with_io_error() {
        use std::io;

        let path = PathBuf::from("/test/file.txt");
        let result: Result<String, io::Error> =
            Err(io::Error::new(io::ErrorKind::NotFound, "file not found"));

        let logged = result.log_context("read file", &path);
        assert!(logged.is_err());
    }
}
