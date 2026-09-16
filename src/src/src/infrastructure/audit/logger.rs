//! Audit logger implementation.
//!
//! This module provides the core audit logging functionality,
//! including the async sink trait and the main logger.

use super::event::AuditEvent;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Async trait for audit event sinks.
///
/// Implement this trait to create custom audit event storage backends.
#[async_trait]
pub trait AuditSink: Send + Sync {
    /// Write an audit event to the sink.
    ///
    /// # Errors
    ///
    /// Returns an error if the event cannot be written.
    async fn log(&self, event: &AuditEvent) -> Result<()>;

    /// Flush any buffered events to persistent storage.
    ///
    /// # Errors
    ///
    /// Returns an error if the flush operation fails.
    async fn flush(&self) -> Result<()>;

    /// Query audit events with optional filters.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of events to return
    /// * `offset` - Number of events to skip (for pagination)
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    async fn query(&self, limit: usize, offset: usize) -> Result<Vec<AuditEvent>>;

    /// Count total number of audit events.
    ///
    /// # Errors
    ///
    /// Returns an error if the count operation fails.
    async fn count(&self) -> Result<usize>;

    /// Close the sink and clean up resources.
    ///
    /// # Errors
    ///
    /// Returns an error if cleanup fails.
    async fn close(&self) -> Result<()>;
}

/// Main audit logger.
///
/// The logger coordinates multiple sinks and provides a unified interface
/// for logging audit events.
pub struct AuditLogger {
    /// Collection of sinks to write events to
    sinks: Arc<RwLock<Vec<Box<dyn AuditSink>>>>,

    /// Whether the logger is enabled
    enabled: Arc<RwLock<bool>>,
}

impl AuditLogger {
    /// Create a new audit logger.
    pub fn new() -> Self {
        Self {
            sinks: Arc::new(RwLock::new(Vec::new())),
            enabled: Arc::new(RwLock::new(true)),
        }
    }

    /// Add a sink to the logger.
    ///
    /// # Arguments
    ///
    /// * `sink` - The sink to add
    pub async fn add_sink(&self, sink: Box<dyn AuditSink>) {
        let mut sinks = self.sinks.write().await;
        sinks.push(sink);
        info!("Audit sink added, total sinks: {}", sinks.len());
    }

    /// Log an audit event to all configured sinks.
    ///
    /// If any sink fails, the error is logged but the function continues
    /// to write to other sinks. This ensures audit events are not lost
    /// due to a single sink failure.
    ///
    /// # Arguments
    ///
    /// * `event` - The audit event to log
    ///
    /// # Errors
    ///
    /// Returns an error only if all sinks fail. Individual sink failures
    /// are logged as warnings.
    pub async fn log(&self, event: AuditEvent) -> Result<()> {
        let enabled = *self.enabled.read().await;
        if !enabled {
            debug!("Audit logging is disabled, skipping event");
            return Ok(());
        }

        // Log to tracing for structured logging integration
        info!(
            event_id = %event.id,
            action = ?event.action,
            result = ?event.result,
            user_id = ?event.user_id,
            resource_id = ?event.resource_id,
            "Audit event logged"
        );

        let sinks = self.sinks.read().await;

        if sinks.is_empty() {
            warn!("No audit sinks configured, event will not be persisted");
            return Ok(());
        }

        let mut errors = Vec::new();
        let mut success_count = 0;

        for (i, sink) in sinks.iter().enumerate() {
            match sink.log(&event).await {
                Ok(()) => {
                    success_count += 1;
                    debug!("Event written to sink {}", i);
                }
                Err(e) => {
                    warn!("Failed to write event to sink {}: {}", i, e);
                    errors.push(e);
                }
            }
        }

        // If all sinks failed, return an error
        if success_count == 0 && !errors.is_empty() {
            error!("All audit sinks failed to write event");
            return Err(AppError::Other(format!(
                "All audit sinks failed: {} errors",
                errors.len()
            )));
        }

        Ok(())
    }

    /// Flush all sinks.
    ///
    /// # Errors
    ///
    /// Returns an error if any sink fails to flush.
    pub async fn flush(&self) -> Result<()> {
        let sinks = self.sinks.read().await;

        for (i, sink) in sinks.iter().enumerate() {
            if let Err(e) = sink.flush().await {
                error!("Failed to flush sink {}: {}", i, e);
                return Err(e);
            }
        }

        debug!("All audit sinks flushed successfully");
        Ok(())
    }

    /// Query audit events from the first available sink.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of events to return
    /// * `offset` - Number of events to skip (for pagination)
    ///
    /// # Errors
    ///
    /// Returns an error if no sinks are configured or if the query fails.
    pub async fn query(&self, limit: usize, offset: usize) -> Result<Vec<AuditEvent>> {
        let sinks = self.sinks.read().await;

        if sinks.is_empty() {
            return Err(AppError::Other("No audit sinks configured".to_string()));
        }

        // Query from the first sink
        let sink = sinks
            .first()
            .ok_or_else(|| AppError::Other("No audit sink available".to_string()))?;
        sink.query(limit, offset).await
    }

    /// Count total audit events from the first available sink.
    ///
    /// # Errors
    ///
    /// Returns an error if no sinks are configured or if the count fails.
    pub async fn count(&self) -> Result<usize> {
        let sinks = self.sinks.read().await;

        if sinks.is_empty() {
            return Err(AppError::Other("No audit sinks configured".to_string()));
        }

        // Count from the first sink
        let sink = sinks
            .first()
            .ok_or_else(|| AppError::Other("No audit sink available".to_string()))?;
        sink.count().await
    }

    /// Enable audit logging.
    pub async fn enable(&self) {
        let mut enabled = self.enabled.write().await;
        *enabled = true;
        info!("Audit logging enabled");
    }

    /// Disable audit logging.
    pub async fn disable(&self) {
        let mut enabled = self.enabled.write().await;
        *enabled = false;
        warn!("Audit logging disabled");
    }

    /// Check if audit logging is enabled.
    pub async fn is_enabled(&self) -> bool {
        *self.enabled.read().await
    }

    /// Close all sinks and clean up resources.
    ///
    /// # Errors
    ///
    /// Returns an error if any sink fails to close.
    pub async fn close(&self) -> Result<()> {
        let sinks = self.sinks.read().await;

        for (i, sink) in sinks.iter().enumerate() {
            if let Err(e) = sink.close().await {
                error!("Failed to close sink {}: {}", i, e);
                return Err(e);
            }
        }

        info!("All audit sinks closed successfully");
        Ok(())
    }
}

impl Default for AuditLogger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::event::{AuditAction, AuditResult};

    struct MockSink {
        should_fail: bool,
    }

    #[async_trait]
    impl AuditSink for MockSink {
        async fn log(&self, _event: &AuditEvent) -> Result<()> {
            if self.should_fail {
                Err(AppError::Other("Mock sink failure".to_string()))
            } else {
                Ok(())
            }
        }

        async fn flush(&self) -> Result<()> {
            Ok(())
        }

        async fn query(&self, _limit: usize, _offset: usize) -> Result<Vec<AuditEvent>> {
            Ok(Vec::new())
        }

        async fn count(&self) -> Result<usize> {
            Ok(0)
        }

        async fn close(&self) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_logger_new() {
        let logger = AuditLogger::new();
        assert!(logger.is_enabled().await);
    }

    #[tokio::test]
    async fn test_logger_enable_disable() {
        let logger = AuditLogger::new();

        logger.disable().await;
        assert!(!logger.is_enabled().await);

        logger.enable().await;
        assert!(logger.is_enabled().await);
    }

    #[tokio::test]
    async fn test_logger_no_sinks() {
        let logger = AuditLogger::new();
        let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success());

        // Should succeed even with no sinks (warning logged)
        let result = logger.log(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_logger_with_successful_sink() {
        let logger = AuditLogger::new();
        logger
            .add_sink(Box::new(MockSink { should_fail: false }))
            .await;

        let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success());
        let result = logger.log(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_logger_with_failing_sink() {
        let logger = AuditLogger::new();
        logger
            .add_sink(Box::new(MockSink { should_fail: true }))
            .await;

        let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success());
        let result = logger.log(event).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_logger_with_mixed_sinks() {
        let logger = AuditLogger::new();
        logger
            .add_sink(Box::new(MockSink { should_fail: false }))
            .await;
        logger
            .add_sink(Box::new(MockSink { should_fail: true }))
            .await;

        let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success());

        // Should succeed because one sink works
        let result = logger.log(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_logger_disabled() {
        let logger = AuditLogger::new();
        logger
            .add_sink(Box::new(MockSink { should_fail: false }))
            .await;
        logger.disable().await;

        let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success());
        let result = logger.log(event).await;

        assert!(result.is_ok());
    }
}
