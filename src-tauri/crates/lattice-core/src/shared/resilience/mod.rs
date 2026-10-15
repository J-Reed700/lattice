//! Retry with exponential backoff, retry predicates and jitter.

pub mod retry;

pub use retry::{retry_with_backoff, RetryConfig};
