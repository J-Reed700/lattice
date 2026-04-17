//! Result type aliases for the shared kernel.
//!
//! Provides convenient type aliases for Result types used throughout
//! the application, reducing boilerplate and ensuring consistency.

/// Standard Result type using the shared AppError
///
/// This is the primary result type for all fallible operations in the application.
///
/// # Examples
///
/// ```rust
/// use vault_desktop::shared::Result;
///
/// fn process_data(input: &str) -> Result<String> {
///     if input.is_empty() {
///         return Err(AppError::InvalidInput("Input cannot be empty".into()));
///     }
///     Ok(input.to_uppercase())
/// }
/// ```
pub type Result<T> = std::result::Result<T, crate::shared::error::AppError>;

/// Result type for async operations
///
/// Convenience alias for async functions that return Result.
pub type AsyncResult<T> = std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>>;
