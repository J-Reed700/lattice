//! Notification port for event broadcasting.
//!
//! This port defines the interface for sending notifications and events
//! to external systems or UI components.
//!
//! # Purpose
//!
//! - Abstracts notification mechanism implementation
//! - Enables decoupled event-driven architecture
//! - Supports testing with mock notification receivers
//! - Allows multiple notification channels (UI, logs, webhooks, etc.)
//!
//! # Infrastructure Implementations
//!
//! - `TauriEventAdapter` - Send events to Tauri frontend via IPC
//! - `LogNotificationAdapter` - Log events to observability system
//! - `WebhookAdapter` - Send events to external webhooks
//! - `MockNotificationAdapter` - Capture events for testing
//!
//! # Example Usage
//!
//! ```rust
//! use crate::application::ports::NotificationPort;
//! use serde_json::json;
//!
//! fn notify_indexing_complete(
//!     notifier: &impl NotificationPort,
//!     doc_id: &str,
//! ) -> Result<()> {
//!     notifier.notify(
//!         "indexing:complete",
//!         json!({ "document_id": doc_id }),
//!     )
//! }
//! ```

use crate::shared::result::Result;
use serde_json::Value;

/// Type alias for notification callback functions.
pub type NotificationCallback = Box<dyn Fn(&str, &Value) + Send + Sync>;

/// Port for notification and event broadcasting.
///
/// Implementations must:
/// - Support arbitrary event types and payloads
/// - Handle notification failures gracefully (log but don't crash)
/// - Be non-blocking or async where appropriate
/// - Be thread-safe (`Send + Sync`)
pub trait NotificationPort: Send + Sync {
    /// Send a notification for an event.
    ///
    /// Events are identified by a string type and carry a JSON payload.
    /// The event type should follow a namespaced convention like:
    /// - `indexing:started`
    /// - `indexing:progress`
    /// - `indexing:complete`
    /// - `search:results`
    /// - `error:critical`
    ///
    /// # Arguments
    ///
    /// * `event` - The event type identifier (namespaced string)
    /// * `payload` - The event data as a JSON value
    ///
    /// # Errors
    ///
    /// Implementations should be lenient with errors. Notification failures
    /// should not break application logic. Common practice:
    /// - Log errors instead of propagating them
    /// - Return `Ok(())` even if delivery fails
    /// - Only return `Err` for critical failures (e.g., invalid payload)
    ///
    /// # Example
    ///
    /// ```rust
    /// use serde_json::json;
    ///
    /// // Simple event
    /// notifier.notify("indexing:started", json!({
    ///     "document_id": "doc-123"
    /// }))?;
    ///
    /// // Progress event
    /// notifier.notify("indexing:progress", json!({
    ///     "current": 5,
    ///     "total": 10,
    ///     "percentage": 50.0
    /// }))?;
    ///
    /// // Error event
    /// notifier.notify("error:indexing", json!({
    ///     "document_id": "doc-123",
    ///     "error": "File not found"
    /// }))?;
    /// ```
    fn notify(&self, event: &str, payload: Value) -> Result<()>;

    /// Send a batch of notifications.
    ///
    /// More efficient than calling `notify` repeatedly for multiple events.
    ///
    /// # Arguments
    ///
    /// * `notifications` - Slice of (event, payload) tuples
    ///
    /// # Errors
    ///
    /// Same lenient error handling as `notify`. Batch should be best-effort.
    ///
    /// # Example
    ///
    /// ```rust
    /// let events = vec![
    ///     ("indexing:started", json!({"id": "doc-1"})),
    ///     ("indexing:started", json!({"id": "doc-2"})),
    ///     ("indexing:started", json!({"id": "doc-3"})),
    /// ];
    /// notifier.notify_batch(&events)?;
    /// ```
    fn notify_batch(&self, notifications: &[(&str, Value)]) -> Result<()> {
        // Default implementation: call notify for each
        for (event, payload) in notifications {
            self.notify(event, payload.clone())?;
        }
        Ok(())
    }

    /// Subscribe to events of a specific type.
    ///
    /// Optional method for implementations that support subscriptions.
    /// Not all implementations need to support this (e.g., one-way webhooks).
    ///
    /// # Arguments
    ///
    /// * `event_pattern` - Event type or pattern to subscribe to (e.g., "indexing:*")
    /// * `callback` - Function to call when matching events occur
    ///
    /// # Returns
    ///
    /// A subscription handle that can be used to unsubscribe.
    ///
    /// # Example
    ///
    /// ```rust
    /// let subscription = notifier.subscribe("indexing:*", |event, payload| {
    ///     println!("Received {}: {:?}", event, payload);
    /// })?;
    /// ```
    fn subscribe(
        &self,
        _event_pattern: &str,
        _callback: NotificationCallback,
    ) -> Result<SubscriptionHandle> {
        // Default: not supported
        Err(crate::shared::error::AppError::Other(
            "Subscriptions not supported by this notifier".into(),
        ))
    }
}

/// Handle for an event subscription.
///
/// Dropping this handle should unsubscribe from the event.
pub struct SubscriptionHandle {
    /// Unique ID for this subscription
    pub id: String,
}

impl Drop for SubscriptionHandle {
    fn drop(&mut self) {
        // Implementations should clean up subscription when dropped
    }
}

/// Common event types used throughout the application.
///
/// These are conventions, not enforced by the port.
pub mod events {
    /// Indexing lifecycle events
    pub const INDEXING_STARTED: &str = "indexing:started";
    pub const INDEXING_PROGRESS: &str = "indexing:progress";
    pub const INDEXING_COMPLETE: &str = "indexing:complete";
    pub const INDEXING_FAILED: &str = "indexing:failed";

    /// Search events
    pub const SEARCH_STARTED: &str = "search:started";
    pub const SEARCH_RESULTS: &str = "search:results";
    pub const SEARCH_FAILED: &str = "search:failed";

    /// Q&A events
    pub const QA_STARTED: &str = "qa:started";
    pub const QA_STREAMING: &str = "qa:streaming";
    pub const QA_COMPLETE: &str = "qa:complete";
    pub const QA_FAILED: &str = "qa:failed";

    /// System events
    pub const ERROR_CRITICAL: &str = "error:critical";
    pub const ERROR_WARNING: &str = "error:warning";
    pub const SYSTEM_READY: &str = "system:ready";
    pub const SYSTEM_SHUTDOWN: &str = "system:shutdown";
}
