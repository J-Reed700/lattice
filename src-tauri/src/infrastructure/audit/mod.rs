//! Audit logging system for the Tauri application.
//!
//! This module provides comprehensive audit logging capabilities for tracking
//! security-relevant events, user actions, and system operations.
//!
//! # Features
//!
//! - **Event Tracking**: Track various actions like file indexing, searches, credential access, etc.
//! - **Multiple Sinks**: Support for SQLite and in-memory storage backends
//! - **Async Support**: Fully asynchronous API using tokio
//! - **Structured Logging**: Integration with tracing for structured log output
//! - **Easy-to-use Macros**: Convenient macros for common logging patterns
//!
//! # Examples
//!
//! ## Basic Usage
//!
//! ```no_run
//! use lattice::audit::{AuditLogger, AuditEvent, AuditAction, AuditResult};
//! use lattice::audit::sinks::MemoryAuditSink;
//!
//! #[tokio::main]
//! async fn main() {
//!     // Create logger and add a sink
//!     let logger = AuditLogger::new();
//!     logger.add_sink(Box::new(MemoryAuditSink::new(1000))).await;
//!
//!     // Log an event
//!     let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success())
//!         .with_resource_id("/path/to/file.txt")
//!         .with_metadata("size", "1024");
//!
//!     logger.log(event).await.unwrap();
//! }
//! ```
//!
//! ## Using Macros
//!
//! ```no_run
//! use lattice::audit::{audit_success, audit_failure, get_audit_logger};
//! use lattice::audit::AuditAction;
//!
//! #[tokio::main]
//! async fn main() {
//!     // Get global logger instance
//!     let logger = get_audit_logger();
//!
//!     // Log success
//!     audit_success!(logger, AuditAction::FileIndexed, "/path/to/file.txt",
//!         "size" => "1024",
//!         "type" => "text/plain"
//!     ).await;
//!
//!     // Log failure
//!     audit_failure!(logger, AuditAction::FileIndexed, "/path/to/file.txt",
//!         "Permission denied"
//!     ).await;
//! }
//! ```

pub mod event;
pub mod logger;
pub mod sinks;

pub use event::{AuditAction, AuditEvent, AuditEventBuilder, AuditResult};
pub use logger::{AuditLogger, AuditSink};

use once_cell::sync::Lazy;
use std::sync::Arc;

/// Global audit logger instance.
///
/// This is a singleton logger that can be accessed from anywhere in the application.
/// It must be initialized before use with `init_audit_logger()`.
static GLOBAL_AUDIT_LOGGER: Lazy<Arc<AuditLogger>> = Lazy::new(|| Arc::new(AuditLogger::new()));

/// Get a reference to the global audit logger.
///
/// # Examples
///
/// ```
/// use lattice::audit::get_audit_logger;
///
/// let logger = get_audit_logger();
/// ```
pub fn get_audit_logger() -> Arc<AuditLogger> {
    Arc::clone(&GLOBAL_AUDIT_LOGGER)
}

/// Initialize the global audit logger with the given sinks.
///
/// This function should be called once during application startup.
///
/// # Examples
///
/// ```no_run
/// use lattice::audit::{init_audit_logger, sinks::MemoryAuditSink};
///
/// #[tokio::main]
/// async fn main() {
///     init_audit_logger(vec![
///         Box::new(MemoryAuditSink::new(1000))
///     ]).await;
/// }
/// ```
pub async fn init_audit_logger(sinks: Vec<Box<dyn AuditSink>>) {
    let logger = get_audit_logger();
    for sink in sinks {
        logger.add_sink(sink).await;
    }
}

/// Log a successful audit event.
///
/// # Examples
///
/// ```no_run
/// # use lattice::audit::{audit_success, get_audit_logger, AuditAction};
/// # #[tokio::main]
/// # async fn main() {
/// let logger = get_audit_logger();
///
/// // Simple success log
/// audit_success!(logger, AuditAction::FileIndexed, "/path/to/file.txt").await;
///
/// // With metadata
/// audit_success!(logger, AuditAction::SearchPerformed, "query text",
///     "results" => "10",
///     "duration_ms" => "250"
/// ).await;
/// # }
/// ```
#[macro_export]
macro_rules! audit_success {
    ($logger:expr, $action:expr, $resource:expr) => {
        $logger
            .log(
                $crate::audit::AuditEvent::new($action, $crate::audit::AuditResult::success())
                    .with_resource_id($resource),
            )
    };

    ($logger:expr, $action:expr, $resource:expr, $($key:expr => $value:expr),* $(,)?) => {
        $logger
            .log(
                $crate::audit::AuditEvent::new($action, $crate::audit::AuditResult::success())
                    .with_resource_id($resource)
                    $(.with_metadata($key, $value))*
            )
    };
}

/// Log a failed audit event.
///
/// # Examples
///
/// ```no_run
/// # use lattice::audit::{audit_failure, get_audit_logger, AuditAction};
/// # #[tokio::main]
/// # async fn main() {
/// let logger = get_audit_logger();
///
/// // Simple failure log
/// audit_failure!(logger, AuditAction::FileIndexed, "/path/to/file.txt",
///     "File not found"
/// ).await;
///
/// // With metadata
/// audit_failure!(logger, AuditAction::FileIndexed, "/path/to/file.txt",
///     "Permission denied",
///     "error_code" => "403",
///     "user" => "unknown"
/// ).await;
/// # }
/// ```
#[macro_export]
macro_rules! audit_failure {
    ($logger:expr, $action:expr, $resource:expr, $reason:expr) => {
        $logger
            .log(
                $crate::audit::AuditEvent::new(
                    $action,
                    $crate::audit::AuditResult::failure($reason),
                )
                .with_resource_id($resource),
            )
    };

    ($logger:expr, $action:expr, $resource:expr, $reason:expr, $($key:expr => $value:expr),* $(,)?) => {
        $logger
            .log(
                $crate::audit::AuditEvent::new(
                    $action,
                    $crate::audit::AuditResult::failure($reason),
                )
                .with_resource_id($resource)
                $(.with_metadata($key, $value))*
            )
    };
}

/// Log a denied audit event.
///
/// # Examples
///
/// ```no_run
/// # use lattice::audit::{audit_denied, get_audit_logger, AuditAction};
/// # #[tokio::main]
/// # async fn main() {
/// let logger = get_audit_logger();
///
/// // Simple denied log
/// audit_denied!(logger, AuditAction::CredentialAccessed, "api_key",
///     "Insufficient permissions"
/// ).await;
///
/// // With metadata
/// audit_denied!(logger, AuditAction::ConfigChanged, "security_settings",
///     "Admin role required",
///     "user_role" => "user",
///     "required_role" => "admin"
/// ).await;
/// # }
/// ```
#[macro_export]
macro_rules! audit_denied {
    ($logger:expr, $action:expr, $resource:expr, $reason:expr) => {
        $logger
            .log(
                $crate::audit::AuditEvent::new(
                    $action,
                    $crate::audit::AuditResult::denied($reason),
                )
                .with_resource_id($resource),
            )
    };

    ($logger:expr, $action:expr, $resource:expr, $reason:expr, $($key:expr => $value:expr),* $(,)?) => {
        $logger
            .log(
                $crate::audit::AuditEvent::new(
                    $action,
                    $crate::audit::AuditResult::denied($reason),
                )
                .with_resource_id($resource)
                $(.with_metadata($key, $value))*
            )
    };
}

/// Log a custom audit event with full control.
///
/// # Examples
///
/// ```no_run
/// # use lattice::audit::{audit_event, get_audit_logger, AuditAction, AuditResult};
/// # #[tokio::main]
/// # async fn main() {
/// let logger = get_audit_logger();
///
/// audit_event!(logger,
///     action: AuditAction::SearchPerformed,
///     result: AuditResult::success(),
///     resource: "semantic search",
///     user: "user123",
///     metadata: {
///         "query" => "machine learning",
///         "results" => "25",
///         "duration_ms" => "180"
///     }
/// ).await;
/// # }
/// ```
#[macro_export]
macro_rules! audit_event {
    ($logger:expr,
        action: $action:expr,
        result: $result:expr,
        resource: $resource:expr
        $(, user: $user:expr)?
        $(, metadata: { $($key:expr => $value:expr),* $(,)? })?
        $(,)?
    ) => {
        {
            let mut event = $crate::audit::AuditEvent::new($action, $result)
                .with_resource_id($resource);

            $(
                event = event.with_user_id($user);
            )?

            $(
                $(
                    event = event.with_metadata($key, $value);
                )*
            )?

            $logger.log(event)
        }
    };
}
