//! Retry utilities for transient failure handling.
//!
//! Provides configurable retry mechanisms with exponential backoff and jitter
//! for handling transient failures in network requests, database operations,
//! and other potentially failing operations.

use rand::Rng;
use std::time::Duration;
use tokio::time::sleep;
use tracing;

/// Retry configuration with exponential backoff settings.
///
/// # Examples
///
/// ```rust
/// use vault_desktop::utils::RetryConfig;
/// use std::time::Duration;
///
/// // Default configuration (3 attempts, 100ms initial delay)
/// let config = RetryConfig::default();
///
/// // Aggressive retry for critical operations
/// let config = RetryConfig::aggressive();
///
/// // Conservative retry for non-critical operations
/// let config = RetryConfig::conservative();
///
/// // Custom configuration
/// let config = RetryConfig {
///     max_attempts: 5,
///     initial_delay: Duration::from_millis(200),
///     max_delay: Duration::from_secs(10),
///     backoff_multiplier: 2.0,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of attempts (including first try)
    pub max_attempts: usize,
    /// Initial delay between retries
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    /// Aggressive retry configuration for critical operations.
    ///
    /// Use for operations that are expensive to fail or must eventually succeed:
    /// - LLM API calls (expensive, rate-limited)
    /// - Critical database writes
    /// - Authentication requests
    ///
    /// Configuration: 5 attempts, 50ms initial delay, up to 10s max delay
    pub fn aggressive() -> Self {
        Self {
            max_attempts: 5,
            initial_delay: Duration::from_millis(50),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
        }
    }

    /// Conservative retry configuration for non-critical operations.
    ///
    /// Use for operations that are cheap to retry or can afford to fail:
    /// - Cache lookups
    /// - Non-critical reads
    /// - Background tasks
    ///
    /// Configuration: 2 attempts, 500ms initial delay, up to 2s max delay
    pub fn conservative() -> Self {
        Self {
            max_attempts: 2,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(2),
            backoff_multiplier: 2.0,
        }
    }

    /// Quick retry configuration for fast operations.
    ///
    /// Use for operations that should complete quickly:
    /// - Health checks
    /// - Quick database queries
    ///
    /// Configuration: 3 attempts, 50ms initial delay, up to 1s max delay
    pub fn quick() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(50),
            max_delay: Duration::from_secs(1),
            backoff_multiplier: 1.5,
        }
    }
}

/// Retry a future with exponential backoff.
///
/// # Arguments
///
/// * `config` - Retry configuration
/// * `operation` - Async function to retry (must return Result)
/// * `is_retryable` - Function to determine if error is retryable
///
/// # Returns
///
/// Returns the successful result or the last error after all retries exhausted.
///
/// # Examples
///
/// ```rust
/// use vault_desktop::utils::{retry_with_backoff, RetryConfig};
/// use std::io;
///
/// async fn example() -> Result<String, io::Error> {
///     retry_with_backoff(
///         RetryConfig::default(),
///         || async {
///             // Your operation here
///             Ok("success".to_string())
///         },
///         |e| {
///             // Retry on connection errors
///             e.kind() == io::ErrorKind::ConnectionRefused
///         }
///     ).await
/// }
/// ```
pub async fn retry_with_backoff<F, Fut, T, E, R>(
    config: RetryConfig,
    mut operation: F,
    is_retryable: R,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    R: Fn(&E) -> bool,
{
    let mut attempt = 1;
    let mut delay = config.initial_delay;

    loop {
        match operation().await {
            Ok(result) => {
                if attempt > 1 {
                    tracing::info!(attempt = attempt, "Operation succeeded after retry");
                }
                return Ok(result);
            }
            Err(e) => {
                if attempt >= config.max_attempts {
                    tracing::error!(
                        attempt = attempt,
                        max_attempts = config.max_attempts,
                        "Operation failed after max retries"
                    );
                    return Err(e);
                }

                if !is_retryable(&e) {
                    tracing::debug!(
                        attempt = attempt,
                        "Error not retryable, failing immediately"
                    );
                    return Err(e);
                }

                tracing::warn!(
                    attempt = attempt,
                    delay_ms = delay.as_millis(),
                    max_attempts = config.max_attempts,
                    "Operation failed, retrying after delay"
                );

                sleep(delay).await;

                // Exponential backoff
                delay = Duration::from_millis(
                    ((delay.as_millis() as f64) * config.backoff_multiplier) as u64,
                );
                delay = delay.min(config.max_delay);

                attempt += 1;
            }
        }
    }
}

/// Simpler retry for operations where all errors should be retried.
///
/// This is a convenience wrapper around `retry_with_backoff` that retries
/// all errors with default configuration.
///
/// # Examples
///
/// ```rust
/// use vault_desktop::utils::retry_async;
///
/// async fn example() -> Result<String, String> {
///     retry_async(|| async {
///         // Your operation here
///         Ok("success".to_string())
///     }).await
/// }
/// ```
pub async fn retry_async<F, Fut, T, E>(operation: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    retry_with_backoff(
        RetryConfig::default(),
        operation,
        |_| true, // Retry all errors
    )
    .await
}

/// Retry with custom jitter to prevent thundering herd.
///
/// Adds random jitter (±20%) to delay times to prevent multiple clients
/// from retrying at the exact same time when a service recovers.
///
/// # Arguments
///
/// * `config` - Retry configuration
/// * `operation` - Async function to retry (must return Result)
/// * `is_retryable` - Function to determine if error is retryable
///
/// # Examples
///
/// ```rust
/// use vault_desktop::utils::{retry_with_jitter, RetryConfig};
///
/// async fn example() -> Result<String, reqwest::Error> {
///     retry_with_jitter(
///         RetryConfig::aggressive(),
///         || async {
///             reqwest::get("https://api.example.com/health")
///                 .await?
///                 .text()
///                 .await
///         },
///         |e| e.is_connect() || e.is_timeout()
///     ).await
/// }
/// ```
pub async fn retry_with_jitter<F, Fut, T, E, R>(
    config: RetryConfig,
    mut operation: F,
    is_retryable: R,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    R: Fn(&E) -> bool,
{
    let mut rng = rand::thread_rng();

    let mut attempt = 1;
    let mut delay = config.initial_delay;

    loop {
        match operation().await {
            Ok(result) => {
                if attempt > 1 {
                    tracing::info!(attempt = attempt, "Operation succeeded after retry");
                }
                return Ok(result);
            }
            Err(e) => {
                if attempt >= config.max_attempts {
                    tracing::error!(
                        attempt = attempt,
                        max_attempts = config.max_attempts,
                        "Operation failed after max retries"
                    );
                    return Err(e);
                }

                if !is_retryable(&e) {
                    tracing::debug!(
                        attempt = attempt,
                        "Error not retryable, failing immediately"
                    );
                    return Err(e);
                }

                // Add jitter: ±20% of delay
                let jitter = rng.gen_range(-0.2..=0.2);
                let jittered_delay =
                    Duration::from_millis(((delay.as_millis() as f64) * (1.0 + jitter)) as u64);

                tracing::warn!(
                    attempt = attempt,
                    delay_ms = jittered_delay.as_millis(),
                    max_attempts = config.max_attempts,
                    "Operation failed, retrying with jitter"
                );

                sleep(jittered_delay).await;

                // Exponential backoff
                delay = Duration::from_millis(
                    ((delay.as_millis() as f64) * config.backoff_multiplier) as u64,
                );
                delay = delay.min(config.max_delay);

                attempt += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_succeeds_on_first_try() {
        let result = retry_with_backoff(
            RetryConfig::default(),
            || async { Ok::<_, String>("success") },
            |_| true,
        )
        .await;

        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn test_retry_succeeds_on_second_try() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = retry_with_backoff(
            RetryConfig::default(),
            || {
                let counter = counter_clone.clone();
                async move {
                    let count = counter.fetch_add(1, Ordering::SeqCst);
                    if count == 0 {
                        Err("first failure")
                    } else {
                        Ok("success")
                    }
                }
            },
            |_| true,
        )
        .await;

        assert_eq!(result.unwrap(), "success");
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_retry_exhausts_attempts() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = retry_with_backoff(
            RetryConfig {
                max_attempts: 3,
                initial_delay: Duration::from_millis(10),
                max_delay: Duration::from_millis(100),
                backoff_multiplier: 2.0,
            },
            || {
                let counter = counter_clone.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Err::<(), _>("always fails")
                }
            },
            |_| true,
        )
        .await;

        assert_eq!(result.unwrap_err(), "always fails");
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_non_retryable_error() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = retry_with_backoff(
            RetryConfig::default(),
            || {
                let counter = counter_clone.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Err::<(), _>("fatal error")
                }
            },
            |e: &_| *e != "fatal error",
        )
        .await;

        assert_eq!(result.unwrap_err(), "fatal error");
        // Should only attempt once since error is not retryable
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_async_convenience() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = retry_async(|| {
            let counter = counter_clone.clone();
            async move {
                let count = counter.fetch_add(1, Ordering::SeqCst);
                if count == 0 {
                    Err("first failure")
                } else {
                    Ok("success")
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn test_exponential_backoff_timing() {
        use std::time::Instant;

        let start = Instant::now();
        let _ = retry_with_backoff(
            RetryConfig {
                max_attempts: 3,
                initial_delay: Duration::from_millis(50),
                max_delay: Duration::from_secs(1),
                backoff_multiplier: 2.0,
            },
            || async { Err::<(), _>("fail") },
            |_| true,
        )
        .await;

        let elapsed = start.elapsed();
        // Should take at least: 50ms + 100ms = 150ms (two delays)
        assert!(elapsed.as_millis() >= 150);
    }

    #[tokio::test]
    async fn test_aggressive_config() {
        let config = RetryConfig::aggressive();
        assert_eq!(config.max_attempts, 5);
        assert_eq!(config.initial_delay, Duration::from_millis(50));
        assert_eq!(config.max_delay, Duration::from_secs(10));
    }

    #[tokio::test]
    async fn test_conservative_config() {
        let config = RetryConfig::conservative();
        assert_eq!(config.max_attempts, 2);
        assert_eq!(config.initial_delay, Duration::from_millis(500));
        assert_eq!(config.max_delay, Duration::from_secs(2));
    }

    #[tokio::test]
    async fn test_quick_config() {
        let config = RetryConfig::quick();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_delay, Duration::from_millis(50));
        assert_eq!(config.max_delay, Duration::from_secs(1));
    }
}
