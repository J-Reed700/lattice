//! SecurityProvider - Security context, rate limiting, and input validation
//!
//! This module provides a centralized security layer with:
//! - Rate limiting for resource-intensive operations
//! - Input validation for all user inputs
//! - Audit logging for security events
//!
//! # Example
//!
//! ```rust
//! use sqlx::SqlitePool;
//! use std::time::Duration;
//!
//! let pool = SqlitePool::connect("sqlite::memory:").await?;
//! let security = SecurityProviderBuilder::new()
//!     .with_pool(pool)
//!     .with_search_rate_limit(100, Duration::from_secs(60))
//!     .build()?;
//!
//! // Access rate limiters
//! security.rate_limiters().search.check_rate_limit("user123").await?;
//!
//! // Validate input
//! let validated = security.input_validator().validate_search_query("test query")?;
//!
//! // Log audit event
//! let event = AuditEvent::new(AuditAction::Search, AuditResult::success());
//! security.audit_logger().log(event).await?;
//! ```

use crate::infrastructure::audit::logger::{AuditLogger, AuditSink};
use crate::infrastructure::audit::sinks::sqlite::SqliteAuditSink;
use crate::infrastructure::security::input_validator::InputValidator;
use crate::infrastructure::security::rate_limiter::RateLimiter;
use crate::shared::error::{AppError, Result};
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Duration;

/// Rate limiters for different operations
///
/// Provides pre-configured rate limiters for various operations to prevent
/// resource exhaustion (CWE-770 mitigation).
///
/// Default limits:
/// - Search: 100 requests/min
/// - Indexing: 50 requests/min
/// - Embeddings: 200 requests/min
/// - Q&A: 10 requests/min
#[derive(Clone, Debug)]
pub struct RateLimiters {
    /// Search operations rate limiter (100/min)
    pub search: Arc<RateLimiter>,
    /// Indexing operations rate limiter (50/min)
    pub indexing: Arc<RateLimiter>,
    /// Embedding generation rate limiter (200/min)
    pub embeddings: Arc<RateLimiter>,
    /// Question answering rate limiter (10/min)
    pub qa: Arc<RateLimiter>,
}

impl RateLimiters {
    /// Create rate limiters with default limits
    pub fn default() -> Self {
        Self {
            search: Arc::new(RateLimiter::new(100, 60)),
            indexing: Arc::new(RateLimiter::new(50, 60)),
            embeddings: Arc::new(RateLimiter::new(200, 60)),
            qa: Arc::new(RateLimiter::new(10, 60)),
        }
    }

    /// Create rate limiters with custom limits
    pub fn new(
        search: (usize, Duration),
        indexing: (usize, Duration),
        embeddings: (usize, Duration),
        qa: (usize, Duration),
    ) -> Self {
        Self {
            search: Arc::new(RateLimiter::new(search.0, search.1.as_secs())),
            indexing: Arc::new(RateLimiter::new(indexing.0, indexing.1.as_secs())),
            embeddings: Arc::new(RateLimiter::new(embeddings.0, embeddings.1.as_secs())),
            qa: Arc::new(RateLimiter::new(qa.0, qa.1.as_secs())),
        }
    }
}

/// SecurityProvider manages security controls including rate limiting and validation
///
/// Provides centralized access to:
/// - Rate limiters for different operations
/// - Input validator for user input sanitization
/// - Audit logger for security event tracking
///
/// # Example
///
/// ```rust
/// let security = SecurityProviderBuilder::new()
///     .with_pool(pool)
///     .build()?;
///
/// // Rate limiting
/// security.rate_limiters().search.check_rate_limit("user123").await?;
///
/// // Input validation
/// let query = security.input_validator().validate_search_query("test")?;
///
/// // Audit logging
/// security.audit_logger().log(event).await?;
/// ```
pub struct SecurityProvider {
    /// Rate limiters for different operations
    rate_limiters: RateLimiters,
    /// Input validator for sanitization
    input_validator: Arc<InputValidator>,
    /// Audit logger for security events
    audit_logger: Arc<AuditLogger>,
}

impl SecurityProvider {
    /// Get rate limiters
    pub fn rate_limiters(&self) -> &RateLimiters {
        &self.rate_limiters
    }

    /// Get input validator
    pub fn input_validator(&self) -> Arc<InputValidator> {
        Arc::clone(&self.input_validator)
    }

    /// Get audit logger
    pub fn audit_logger(&self) -> Arc<AuditLogger> {
        Arc::clone(&self.audit_logger)
    }
}

/// Builder for SecurityProvider
///
/// Provides a fluent API for configuring security settings.
///
/// # Example
///
/// ```rust
/// let provider = SecurityProviderBuilder::new()
///     .with_pool(pool)
///     .with_search_rate_limit(200, Duration::from_secs(60))
///     .with_indexing_rate_limit(100, Duration::from_secs(60))
///     .build()?;
/// ```
pub struct SecurityProviderBuilder {
    pool: Option<SqlitePool>,
    search_rate_limit: Option<(usize, Duration)>,
    indexing_rate_limit: Option<(usize, Duration)>,
    embeddings_rate_limit: Option<(usize, Duration)>,
    qa_rate_limit: Option<(usize, Duration)>,
}

impl SecurityProviderBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            pool: None,
            search_rate_limit: None,
            indexing_rate_limit: None,
            embeddings_rate_limit: None,
            qa_rate_limit: None,
        }
    }

    /// Set database pool for audit logging
    pub fn with_pool(mut self, pool: SqlitePool) -> Self {
        self.pool = Some(pool);
        self
    }

    /// Set custom search rate limit
    ///
    /// # Arguments
    ///
    /// * `max_requests` - Maximum requests allowed
    /// * `window` - Time window for rate limiting
    pub fn with_search_rate_limit(mut self, max_requests: usize, window: Duration) -> Self {
        self.search_rate_limit = Some((max_requests, window));
        self
    }

    /// Set custom indexing rate limit
    ///
    /// # Arguments
    ///
    /// * `max_requests` - Maximum requests allowed
    /// * `window` - Time window for rate limiting
    pub fn with_indexing_rate_limit(mut self, max_requests: usize, window: Duration) -> Self {
        self.indexing_rate_limit = Some((max_requests, window));
        self
    }

    /// Set custom embeddings rate limit
    ///
    /// # Arguments
    ///
    /// * `max_requests` - Maximum requests allowed
    /// * `window` - Time window for rate limiting
    pub fn with_embeddings_rate_limit(mut self, max_requests: usize, window: Duration) -> Self {
        self.embeddings_rate_limit = Some((max_requests, window));
        self
    }

    /// Set custom Q&A rate limit
    ///
    /// # Arguments
    ///
    /// * `max_requests` - Maximum requests allowed
    /// * `window` - Time window for rate limiting
    pub fn with_qa_rate_limit(mut self, max_requests: usize, window: Duration) -> Self {
        self.qa_rate_limit = Some((max_requests, window));
        self
    }

    /// Build the SecurityProvider
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Database pool is not provided
    /// - Audit sink creation fails
    pub async fn build(self) -> Result<SecurityProvider> {
        let pool = self
            .pool
            .ok_or_else(|| AppError::Other("Database pool is required".to_string()))?;

        // Create rate limiters with custom or default limits
        let rate_limiters = if self.search_rate_limit.is_some()
            || self.indexing_rate_limit.is_some()
            || self.embeddings_rate_limit.is_some()
            || self.qa_rate_limit.is_some()
        {
            RateLimiters::new(
                self.search_rate_limit
                    .unwrap_or((100, Duration::from_secs(60))),
                self.indexing_rate_limit
                    .unwrap_or((50, Duration::from_secs(60))),
                self.embeddings_rate_limit
                    .unwrap_or((200, Duration::from_secs(60))),
                self.qa_rate_limit
                    .unwrap_or((10, Duration::from_secs(60))),
            )
        } else {
            RateLimiters::default()
        };

        // Create input validator
        let input_validator = Arc::new(InputValidator::new());

        // Create audit logger with SQLite sink
        let audit_logger = Arc::new(AuditLogger::new());
        let sqlite_sink = SqliteAuditSink::new(pool).await?;
        audit_logger.add_sink(Box::new(sqlite_sink)).await;

        Ok(SecurityProvider {
            rate_limiters,
            input_validator,
            audit_logger,
        })
    }
}

impl Default for SecurityProviderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::audit::event::{AuditAction, AuditResult};
    use crate::infrastructure::audit::event::AuditEvent;

    async fn create_test_pool() -> SqlitePool {
        SqlitePool::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn test_security_provider_default_limits() {
        let pool = create_test_pool().await;
        let provider = SecurityProviderBuilder::new()
            .with_pool(pool)
            .build()
            .await
            .unwrap();

        // Verify rate limiters exist
        let _search = &provider.rate_limiters().search;
        let _indexing = &provider.rate_limiters().indexing;
        let _embeddings = &provider.rate_limiters().embeddings;
        let _qa = &provider.rate_limiters().qa;

        // Verify input validator exists
        let _validator = provider.input_validator();

        // Verify audit logger exists
        let _logger = provider.audit_logger();
    }

    #[tokio::test]
    async fn test_security_provider_custom_limits() {
        let pool = create_test_pool().await;
        let provider = SecurityProviderBuilder::new()
            .with_pool(pool)
            .with_search_rate_limit(10, Duration::from_secs(60))
            .with_indexing_rate_limit(5, Duration::from_secs(60))
            .with_embeddings_rate_limit(20, Duration::from_secs(60))
            .with_qa_rate_limit(2, Duration::from_secs(60))
            .build()
            .await
            .unwrap();

        // Verify custom limits are applied (test by hitting limit)
        let search_limiter = &provider.rate_limiters().search;

        // Should allow 10 requests
        for _ in 0..10 {
            search_limiter
                .check_rate_limit("test_user")
                .await
                .expect("Should allow request");
        }

        // 11th request should fail
        let result = search_limiter.check_rate_limit("test_user").await;
        assert!(
            result.is_err(),
            "Should hit rate limit after 10 requests"
        );
    }

    #[tokio::test]
    async fn test_security_provider_builder_missing_pool() {
        let result = SecurityProviderBuilder::new().build().await;
        assert!(
            result.is_err(),
            "Should fail without database pool"
        );
    }

    #[tokio::test]
    async fn test_rate_limiters_default() {
        let limiters = RateLimiters::default();

        // Test that limiters exist and can check limits
        let result = limiters.search.check_rate_limit("test").await;
        assert!(result.is_ok(), "Default search limiter should work");

        let result = limiters.indexing.check_rate_limit("test").await;
        assert!(result.is_ok(), "Default indexing limiter should work");

        let result = limiters.embeddings.check_rate_limit("test").await;
        assert!(result.is_ok(), "Default embeddings limiter should work");

        let result = limiters.qa.check_rate_limit("test").await;
        assert!(result.is_ok(), "Default Q&A limiter should work");
    }

    #[tokio::test]
    async fn test_rate_limiters_custom() {
        let limiters = RateLimiters::new(
            (5, Duration::from_secs(60)),
            (3, Duration::from_secs(60)),
            (10, Duration::from_secs(60)),
            (1, Duration::from_secs(60)),
        );

        // Test search limiter (5 requests)
        for _ in 0..5 {
            limiters
                .search
                .check_rate_limit("test")
                .await
                .expect("Should allow");
        }
        assert!(
            limiters.search.check_rate_limit("test").await.is_err(),
            "Should hit limit"
        );

        // Test Q&A limiter (1 request)
        limiters
            .qa
            .check_rate_limit("test2")
            .await
            .expect("Should allow");
        assert!(
            limiters.qa.check_rate_limit("test2").await.is_err(),
            "Should hit limit"
        );
    }

    #[tokio::test]
    async fn test_input_validator_integration() {
        let pool = create_test_pool().await;
        let provider = SecurityProviderBuilder::new()
            .with_pool(pool)
            .build()
            .await
            .unwrap();

        let validator = provider.input_validator();

        // Test valid query
        let result = validator.validate_search_query("test query");
        assert!(result.is_ok(), "Should validate valid query");

        // Test empty query
        let result = validator.validate_search_query("");
        assert!(result.is_err(), "Should reject empty query");

        // Test trimming
        let result = validator.validate_search_query("  test  ").unwrap();
        assert_eq!(result, "test", "Should trim whitespace");
    }

    #[tokio::test]
    async fn test_audit_logger_integration() {
        let pool = create_test_pool().await;
        let provider = SecurityProviderBuilder::new()
            .with_pool(pool)
            .build()
            .await
            .unwrap();

        let logger = provider.audit_logger();

        // Test logging an event
        let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success());
        let result = logger.log(event).await;
        assert!(result.is_ok(), "Should log event successfully");

        // Test querying events
        let events = logger.query(10, 0).await;
        assert!(events.is_ok(), "Should query events successfully");
    }

    #[tokio::test]
    async fn test_builder_fluent_api() {
        let pool = create_test_pool().await;

        // Test method chaining
        let provider = SecurityProviderBuilder::new()
            .with_pool(pool)
            .with_search_rate_limit(50, Duration::from_secs(30))
            .with_indexing_rate_limit(25, Duration::from_secs(30))
            .with_embeddings_rate_limit(100, Duration::from_secs(30))
            .with_qa_rate_limit(5, Duration::from_secs(30))
            .build()
            .await
            .unwrap();

        // Verify all components are accessible
        let _limiters = provider.rate_limiters();
        let _validator = provider.input_validator();
        let _logger = provider.audit_logger();
    }

    #[tokio::test]
    async fn test_security_provider_arc_sharing() {
        let pool = create_test_pool().await;
        let provider = SecurityProviderBuilder::new()
            .with_pool(pool)
            .build()
            .await
            .unwrap();

        // Test that Arc components can be cloned
        let validator1 = provider.input_validator();
        let validator2 = provider.input_validator();

        // Both should work
        let result1 = validator1.validate_search_query("test1");
        let result2 = validator2.validate_search_query("test2");

        assert!(result1.is_ok());
        assert!(result2.is_ok());
    }
}
