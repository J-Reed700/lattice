use std::error::Error;
use std::fmt;

/// SECURITY FIX: Sanitize error messages (CWE-209)
/// Prevents information disclosure through error messages
pub struct SafeError {
    user_message: String,
    internal_message: String,
    #[allow(dead_code)]
    debug_info: Option<String>,
}

impl SafeError {
    pub fn new(user_message: &str, internal_message: String) -> Self {
        Self {
            user_message: user_message.to_string(),
            internal_message,
            debug_info: None,
        }
    }

    pub fn with_debug(mut self, debug_info: String) -> Self {
        self.debug_info = Some(debug_info);
        self
    }
}

impl fmt::Display for SafeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Only show safe message to users
        write!(f, "{}", self.user_message)
    }
}

impl fmt::Debug for SafeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // In debug mode, show internal message for developers
        #[cfg(debug_assertions)]
        write!(
            f,
            "SafeError {{ user: '{}', internal: '{}', debug: {:?} }}",
            self.user_message, self.internal_message, self.debug_info
        )?;

        #[cfg(not(debug_assertions))]
        write!(f, "{}", self.user_message)?;

        Ok(())
    }
}

impl Error for SafeError {}

/// Sanitize error messages before sending to client
pub fn sanitize_error<E: Error>(err: E) -> String {
    let error_str = err.to_string();

    // Remove sensitive patterns
    let mut sanitized = error_str.clone();

    // Remove file paths
    if sanitized.contains("/") || sanitized.contains("\\") {
        sanitized = "An internal error occurred".to_string();
    }

    // Remove stack traces
    if sanitized.contains(" at ") || sanitized.contains("stack:") {
        sanitized = "An internal error occurred".to_string();
    }

    // Remove SQL errors that might reveal schema
    if sanitized.to_lowercase().contains("sql")
        || sanitized.to_lowercase().contains("database")
        || sanitized.contains("column")
        || sanitized.contains("table")
    {
        sanitized = "A database error occurred".to_string();
    }

    // Remove network/connection details
    if sanitized.contains("127.0.0.1")
        || sanitized.contains("localhost")
        || sanitized.contains("port")
    {
        sanitized = "A connection error occurred".to_string();
    }

    // Log the actual error for debugging (but not to client)
    #[cfg(debug_assertions)]
    tracing::error!("Actual error: {}", error_str);

    sanitized
}

/// Map common errors to user-friendly messages
pub fn map_error_to_user_message(error_type: &str) -> &'static str {
    match error_type {
        "FileNotFound" => "The requested file could not be found",
        "PermissionDenied" => "Access denied",
        "InvalidInput" => "Invalid input provided",
        "DatabaseError" => "A database error occurred",
        "NetworkError" => "A network error occurred",
        "AuthenticationFailed" => "Authentication failed",
        "RateLimitExceeded" => "Too many requests. Please try again later",
        "ValidationError" => "Validation failed",
        _ => "An error occurred",
    }
}

/// Macro for safe error handling in commands
#[macro_export]
macro_rules! safe_error {
    ($result:expr, $user_msg:expr) => {
        match $result {
            Ok(val) => Ok(val),
            Err(e) => {
                tracing::error!("Internal error: {}", e);
                Err($user_msg.to_string())
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_error() {
        // File path should be sanitized
        let err = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found: /etc/passwd");
        assert_eq!(sanitize_error(&err), "An internal error occurred");

        // SQL error should be sanitized
        let sql_err = anyhow::anyhow!("SQL error: column 'password' not found in table 'users'");
        assert_eq!(sanitize_error(&*sql_err), "A database error occurred");

        // Network details should be sanitized
        let net_err = anyhow::anyhow!("Connection refused: 127.0.0.1:5432");
        assert_eq!(sanitize_error(&*net_err), "A connection error occurred");
    }

    #[test]
    fn test_safe_error() {
        let safe_err = SafeError::new("File not found", "Failed to open /etc/passwd".to_string());

        // User sees safe message
        assert_eq!(safe_err.to_string(), "File not found");
    }
}
