//! In-memory audit sink implementation.
//!
//! This sink stores audit events in memory, making it ideal for testing
//! and development environments. Events are lost when the application stops.

use crate::infrastructure::audit::event::AuditEvent;
use crate::infrastructure::audit::logger::AuditSink;
use crate::shared::error::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

/// In-memory audit sink.
///
/// Stores audit events in a Vec in memory. This sink is primarily useful
/// for testing and development, as events are not persisted across restarts.
///
/// # Thread Safety
///
/// This sink is thread-safe and can be shared across multiple threads.
pub struct MemoryAuditSink {
    /// In-memory storage for audit events
    events: Arc<RwLock<Vec<AuditEvent>>>,

    /// Maximum number of events to keep in memory
    max_events: usize,
}

impl MemoryAuditSink {
    /// Create a new in-memory audit sink.
    ///
    /// # Arguments
    ///
    /// * `max_events` - Maximum number of events to keep in memory.
    ///   When this limit is reached, oldest events are removed (FIFO).
    ///
    /// # Examples
    ///
    /// ```
    /// use vault_desktop::audit::sinks::memory::MemoryAuditSink;
    ///
    /// let sink = MemoryAuditSink::new(1000);
    /// ```
    pub fn new(max_events: usize) -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
            max_events,
        }
    }

    /// Create a new in-memory audit sink with unlimited capacity.
    ///
    /// # Warning
    ///
    /// Use this with caution as it can lead to unbounded memory growth.
    /// Only suitable for testing or short-lived processes.
    pub fn unlimited() -> Self {
        Self::new(usize::MAX)
    }

    /// Get all events currently stored in memory.
    ///
    /// Returns a snapshot of all events. The returned vector is independent
    /// of the internal storage.
    pub async fn get_all(&self) -> Vec<AuditEvent> {
        let events = self.events.read().await;
        events.clone()
    }

    /// Clear all events from memory.
    pub async fn clear(&self) {
        let mut events = self.events.write().await;
        events.clear();
        debug!("Memory audit sink cleared");
    }

    /// Get the current number of events stored.
    pub async fn len(&self) -> usize {
        let events = self.events.read().await;
        events.len()
    }

    /// Check if the sink is empty.
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Get the maximum capacity of this sink.
    pub fn capacity(&self) -> usize {
        self.max_events
    }
}

impl Default for MemoryAuditSink {
    /// Create a new memory sink with a default capacity of 10,000 events.
    fn default() -> Self {
        Self::new(10_000)
    }
}

#[async_trait]
impl AuditSink for MemoryAuditSink {
    async fn log(&self, event: &AuditEvent) -> Result<()> {
        let mut events = self.events.write().await;

        // Add the new event
        events.push(event.clone());

        // Remove oldest events if we exceed capacity
        if events.len() > self.max_events {
            let overflow = events.len() - self.max_events;
            events.drain(0..overflow);
            debug!(
                "Memory audit sink capacity exceeded, removed {} oldest events",
                overflow
            );
        }

        debug!(
            "Audit event {} logged to memory ({}/{})",
            event.id,
            events.len(),
            self.max_events
        );

        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        // No-op for memory sink as everything is already "flushed"
        debug!("Memory audit sink flush requested (no-op)");
        Ok(())
    }

    async fn query(&self, limit: usize, offset: usize) -> Result<Vec<AuditEvent>> {
        let events = self.events.read().await;

        // Sort by timestamp descending (most recent first)
        let mut sorted_events = events.clone();
        sorted_events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // Apply pagination
        let result = sorted_events.into_iter().skip(offset).take(limit).collect();

        debug!(
            "Queried audit events from memory with limit={}, offset={}",
            limit, offset
        );
        Ok(result)
    }

    async fn count(&self) -> Result<usize> {
        Ok(self.len().await)
    }

    async fn close(&self) -> Result<()> {
        self.clear().await;
        debug!("Memory audit sink closed and cleared");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::event::{AuditAction, AuditResult};

    #[tokio::test]
    async fn test_sink_creation() {
        let sink = MemoryAuditSink::new(100);
        assert_eq!(sink.capacity(), 100);
        assert!(sink.is_empty().await);
    }

    #[tokio::test]
    async fn test_log_event() {
        let sink = MemoryAuditSink::new(100);
        let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success());

        let result = sink.log(&event).await;
        assert!(result.is_ok());
        assert_eq!(sink.len().await, 1);
    }

    #[tokio::test]
    async fn test_capacity_limit() {
        let sink = MemoryAuditSink::new(5);

        // Add 10 events
        for i in 0..10 {
            let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success())
                .with_resource_id(format!("file{}", i));
            sink.log(&event).await.unwrap();
        }

        // Should only have 5 events (newest ones)
        assert_eq!(sink.len().await, 5);

        // Verify we kept the newest events
        let events = sink.get_all().await;
        let resource_ids: Vec<_> = events
            .iter()
            .filter_map(|e| e.resource_id.as_ref())
            .collect();

        // Should have file5 through file9
        assert!(resource_ids.contains(&&"file9".to_string()));
        assert!(!resource_ids.contains(&&"file0".to_string()));
    }

    #[tokio::test]
    async fn test_query_with_pagination() {
        let sink = MemoryAuditSink::new(100);

        // Add 10 events
        for i in 0..10 {
            let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success())
                .with_resource_id(format!("file{}", i));
            sink.log(&event).await.unwrap();
            // Small delay to ensure different timestamps
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        }

        // Query first page
        let first_page = sink.query(5, 0).await.unwrap();
        assert_eq!(first_page.len(), 5);

        // Query second page
        let second_page = sink.query(5, 5).await.unwrap();
        assert_eq!(second_page.len(), 5);

        // Verify ordering (most recent first)
        assert!(first_page[0].timestamp >= first_page[4].timestamp);
    }

    #[tokio::test]
    async fn test_clear() {
        let sink = MemoryAuditSink::new(100);

        // Add events
        for _ in 0..5 {
            let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success());
            sink.log(&event).await.unwrap();
        }

        assert_eq!(sink.len().await, 5);

        // Clear
        sink.clear().await;
        assert_eq!(sink.len().await, 0);
        assert!(sink.is_empty().await);
    }

    #[tokio::test]
    async fn test_count() {
        let sink = MemoryAuditSink::new(100);

        assert_eq!(sink.count().await.unwrap(), 0);

        // Add events
        for _ in 0..3 {
            let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success());
            sink.log(&event).await.unwrap();
        }

        assert_eq!(sink.count().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn test_close() {
        let sink = MemoryAuditSink::new(100);

        // Add events
        for _ in 0..5 {
            let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success());
            sink.log(&event).await.unwrap();
        }

        // Close should clear
        sink.close().await.unwrap();
        assert_eq!(sink.len().await, 0);
    }

    #[tokio::test]
    async fn test_unlimited_sink() {
        let sink = MemoryAuditSink::unlimited();

        // Add many events
        for _ in 0..1000 {
            let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success());
            sink.log(&event).await.unwrap();
        }

        // All should be retained
        assert_eq!(sink.len().await, 1000);
    }

    #[tokio::test]
    async fn test_get_all() {
        let sink = MemoryAuditSink::new(100);

        // Add events
        for i in 0..3 {
            let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success())
                .with_resource_id(format!("file{}", i));
            sink.log(&event).await.unwrap();
        }

        let all_events = sink.get_all().await;
        assert_eq!(all_events.len(), 3);
    }
}
