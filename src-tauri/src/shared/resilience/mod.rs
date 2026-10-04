//! Retry policies and circuit breaking.
//!
//! `retry` supports retry predicates and jitter. `retry_policy` retains the
//! simpler attempt-count policy; their configuration and call signatures differ.

pub mod circuit_breaker;
pub mod retry;
pub mod retry_policy;

pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError};
pub use retry::{retry_with_backoff, RetryConfig};
