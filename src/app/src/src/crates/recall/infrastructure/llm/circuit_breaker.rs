//! Circuit breaker pattern implementation for LLM service resilience.
//!
//! Prevents cascading failures when external LLM services (Anthropic, Ollama) become
//! unavailable or degraded. Implements the standard 3-state circuit breaker pattern:
//! - Closed: Normal operation
//! - Open: Fail fast (service unavailable)
//! - Half-Open: Testing recovery

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

/// Circuit breaker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    /// Normal operation - requests pass through
    Closed,
    /// Service is failing - fail fast
    Open,
    /// Testing if service recovered
    HalfOpen,
}

/// Circuit breaker configuration.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening circuit
    pub failure_threshold: usize,
    /// How long to wait before testing recovery (Open → HalfOpen)
    pub timeout: Duration,
    /// Number of consecutive successes to close circuit (HalfOpen → Closed)
    pub success_threshold: usize,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            timeout: Duration::from_secs(60),
            success_threshold: 2,
        }
    }
}

/// Circuit breaker implementation.
///
/// Thread-safe circuit breaker that protects external LLM service calls from
/// cascading failures. Uses RwLock for efficient concurrent access.
///
/// # Example
///
/// ```no_run
/// use vault_desktop::llm::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let cb = CircuitBreaker::new(CircuitBreakerConfig::default());
///
/// let result = cb.call(async {
///     // Make external API call
///     Ok::<_, String>("response".to_string())
/// }).await;
///
/// match result {
///     Ok(response) => println!("Success: {}", response),
///     Err(e) => eprintln!("Error: {:?}", e),
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitBreakerState>>,
    config: CircuitBreakerConfig,
}

struct CircuitBreakerState {
    current_state: CircuitState,
    failure_count: usize,
    success_count: usize,
    last_failure_time: Option<Instant>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker with the given configuration.
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitBreakerState {
                current_state: CircuitState::Closed,
                failure_count: 0,
                success_count: 0,
                last_failure_time: None,
            })),
            config,
        }
    }

    /// Execute a future with circuit breaker protection.
    ///
    /// # Arguments
    ///
    /// * `f` - Async function to execute
    ///
    /// # Returns
    ///
    /// Returns the result from `f` or a circuit breaker error if the circuit is open.
    ///
    /// # Errors
    ///
    /// - `CircuitBreakerError::Open` - Circuit is open, failing fast
    /// - `CircuitBreakerError::CallFailed` - The underlying call failed
    pub async fn call<F, T, E>(&self, f: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: std::future::Future<Output = Result<T, E>>,
    {
        // Check state and transition if needed (atomic check-and-set to prevent TOCTOU)
        {
            let mut state = self.state.write();

            if state.current_state == CircuitState::Open {
                // Check if timeout has passed (still under write lock for atomicity)
                if let Some(last_failure) = state.last_failure_time {
                    if last_failure.elapsed() >= self.config.timeout {
                        // Transition to HalfOpen (atomic with check)
                        state.current_state = CircuitState::HalfOpen;
                        state.success_count = 0; // Reset success count for fresh test
                        info!("Circuit breaker transitioning to HalfOpen - testing recovery");
                    } else {
                        // Still in Open state, fail fast
                        let remaining = self.config.timeout.saturating_sub(last_failure.elapsed());
                        warn!(
                            "Circuit breaker is open - failing fast (retry in {:?})",
                            remaining
                        );
                        return Err(CircuitBreakerError::Open);
                    }
                } else {
                    // No last_failure_time recorded, shouldn't happen but fail fast
                    return Err(CircuitBreakerError::Open);
                }
            }
            // If Closed or HalfOpen, allow the call to proceed
        }

        // Execute the call
        match f.await {
            Ok(result) => {
                self.record_success();
                Ok(result)
            }
            Err(e) => {
                self.record_failure();
                Err(CircuitBreakerError::CallFailed(e))
            }
        }
    }

    fn record_success(&self) {
        let mut state = self.state.write();

        match state.current_state {
            CircuitState::HalfOpen => {
                state.success_count += 1;
                if state.success_count >= self.config.success_threshold {
                    state.current_state = CircuitState::Closed;
                    state.failure_count = 0;
                    state.success_count = 0;
                    state.last_failure_time = None;
                    info!(
                        "Circuit breaker closed - service recovered (threshold: {})",
                        self.config.success_threshold
                    );
                } else {
                    info!(
                        "Circuit breaker recovery test succeeded ({}/{})",
                        state.success_count, self.config.success_threshold
                    );
                }
            }
            CircuitState::Closed => {
                // Reset failure count on successful call
                state.failure_count = 0;
            }
            CircuitState::Open => {
                // Shouldn't happen, but reset if it does
                state.failure_count = 0;
            }
        }
    }

    fn record_failure(&self) {
        let mut state = self.state.write();

        match state.current_state {
            CircuitState::Closed => {
                state.failure_count += 1;
                if state.failure_count >= self.config.failure_threshold {
                    state.current_state = CircuitState::Open;
                    state.last_failure_time = Some(Instant::now());
                    error!(
                        "Circuit breaker opened - service failing (threshold: {}, timeout: {:?})",
                        self.config.failure_threshold, self.config.timeout
                    );
                } else {
                    warn!(
                        "Circuit breaker failure recorded ({}/{})",
                        state.failure_count, self.config.failure_threshold
                    );
                }
            }
            CircuitState::HalfOpen => {
                state.current_state = CircuitState::Open;
                state.last_failure_time = Some(Instant::now());
                state.success_count = 0;
                error!("Circuit breaker reopened - recovery test failed");
            }
            CircuitState::Open => {
                // Don't update last_failure_time for fast-fail errors
                // This prevents infinite timeout extension when service is permanently down
                // The timeout window should only reset on actual call failures, not circuit-open rejections
            }
        }
    }

    /// Get current state for monitoring.
    ///
    /// Useful for exposing circuit breaker health in APIs or UIs.
    pub fn state(&self) -> CircuitState {
        self.state.read().current_state
    }

    /// Get failure count in current state.
    pub fn failure_count(&self) -> usize {
        self.state.read().failure_count
    }

    /// Get success count in current state.
    pub fn success_count(&self) -> usize {
        self.state.read().success_count
    }

    /// Manually reset the circuit breaker to Closed state.
    ///
    /// Use with caution - typically only for testing or manual intervention.
    pub fn reset(&self) {
        let mut state = self.state.write();
        state.current_state = CircuitState::Closed;
        state.failure_count = 0;
        state.success_count = 0;
        state.last_failure_time = None;
        info!("Circuit breaker manually reset to Closed state");
    }
}

/// Circuit breaker error.
#[derive(Debug)]
pub enum CircuitBreakerError<E> {
    /// Circuit is open - failing fast
    Open,
    /// The underlying call failed
    CallFailed(E),
}

impl<E: std::fmt::Display> std::fmt::Display for CircuitBreakerError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => write!(f, "Circuit breaker is open - service unavailable"),
            Self::CallFailed(e) => write!(f, "Call failed: {}", e),
        }
    }
}

impl<E: std::error::Error> std::error::Error for CircuitBreakerError<E> {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_circuit_starts_closed() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_successful_calls_stay_closed() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());

        for _ in 0..10 {
            let result = cb.call(async { Ok::<_, String>("success") }).await;
            assert!(result.is_ok());
        }

        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_opens_after_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            timeout: Duration::from_secs(60),
            success_threshold: 2,
        };
        let cb = CircuitBreaker::new(config);

        // First 2 failures should keep circuit closed
        for _ in 0..2 {
            let _ = cb.call(async { Err::<String, _>("error") }).await;
            assert_eq!(cb.state(), CircuitState::Closed);
        }

        // 3rd failure should open the circuit
        let _ = cb.call(async { Err::<String, _>("error") }).await;
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[tokio::test]
    async fn test_open_circuit_fails_fast() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout: Duration::from_secs(60),
            success_threshold: 2,
        };
        let cb = CircuitBreaker::new(config);

        // Open the circuit
        let _ = cb.call(async { Err::<String, _>("error") }).await;
        assert_eq!(cb.state(), CircuitState::Open);

        // Next call should fail fast without executing
        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = call_count.clone();

        let result = cb
            .call(async {
                call_count_clone.fetch_add(1, Ordering::SeqCst);
                Ok::<_, String>("success")
            })
            .await;

        assert!(matches!(result, Err(CircuitBreakerError::Open)));
        assert_eq!(call_count.load(Ordering::SeqCst), 0); // Should not execute
    }

    #[tokio::test]
    async fn test_half_open_after_timeout() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout: Duration::from_millis(50),
            success_threshold: 2,
        };
        let cb = CircuitBreaker::new(config);

        // Open the circuit
        let _ = cb.call(async { Err::<String, _>("error") }).await;
        assert_eq!(cb.state(), CircuitState::Open);

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Next call should transition to HalfOpen
        let _ = cb.call(async { Ok::<_, String>("success") }).await;
        // After first success in HalfOpen, should still be HalfOpen (needs 2)
        assert_eq!(cb.state(), CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn test_half_open_closes_after_successes() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout: Duration::from_millis(50),
            success_threshold: 2,
        };
        let cb = CircuitBreaker::new(config);

        // Open the circuit
        let _ = cb.call(async { Err::<String, _>("error") }).await;

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(100)).await;

        // First success -> HalfOpen
        let _ = cb.call(async { Ok::<_, String>("success") }).await;
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        // Second success -> Closed
        let _ = cb.call(async { Ok::<_, String>("success") }).await;
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_half_open_reopens_on_failure() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout: Duration::from_millis(50),
            success_threshold: 2,
        };
        let cb = CircuitBreaker::new(config);

        // Open the circuit
        let _ = cb.call(async { Err::<String, _>("error") }).await;

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Transition to HalfOpen with success
        let _ = cb.call(async { Ok::<_, String>("success") }).await;
        assert_eq!(cb.state(), CircuitState::HalfOpen);

        // Failure should reopen
        let _ = cb.call(async { Err::<String, _>("error") }).await;
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[tokio::test]
    async fn test_reset() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout: Duration::from_secs(60),
            success_threshold: 2,
        };
        let cb = CircuitBreaker::new(config);

        // Open the circuit
        let _ = cb.call(async { Err::<String, _>("error") }).await;
        assert_eq!(cb.state(), CircuitState::Open);

        // Reset should close it
        cb.reset();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert_eq!(cb.failure_count(), 0);
    }
}
