use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub success_threshold: u32,
    pub timeout: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            timeout: Duration::from_secs(60),
        }
    }
}

#[derive(Debug)]
enum State {
    Closed,
    Open { opened_at: Instant },
    HalfOpen { successes: u32 },
}

pub struct CircuitBreaker {
    state: Arc<Mutex<State>>,
    config: CircuitBreakerConfig,
    consecutive_failures: Arc<Mutex<u32>>,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(State::Closed)),
            config,
            consecutive_failures: Arc::new(Mutex::new(0)),
        }
    }

    pub async fn call<T, F, Fut, E>(&self, f: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        let mut state = self.state.lock().await;

        if let State::Open { opened_at } = &*state {
            if opened_at.elapsed() < self.config.timeout {
                return Err(CircuitBreakerError::Open);
            }
            *state = State::HalfOpen { successes: 0 };
        }

        drop(state);

        match f().await {
            Ok(result) => {
                self.on_success().await;
                Ok(result)
            }
            Err(e) => {
                self.on_failure().await;
                Err(CircuitBreakerError::Inner(e))
            }
        }
    }

    async fn on_success(&self) {
        let mut state = self.state.lock().await;
        let mut failures = self.consecutive_failures.lock().await;
        *failures = 0;

        if let State::HalfOpen { successes } = &*state {
            let new_successes = successes + 1;
            if new_successes >= self.config.success_threshold {
                *state = State::Closed;
            } else {
                *state = State::HalfOpen {
                    successes: new_successes,
                };
            }
        }
    }

    async fn on_failure(&self) {
        let mut state = self.state.lock().await;
        let mut failures = self.consecutive_failures.lock().await;
        *failures += 1;

        if *failures >= self.config.failure_threshold {
            *state = State::Open {
                opened_at: Instant::now(),
            };
        }
    }

    pub async fn get_state_info(&self) -> String {
        let state = self.state.lock().await;
        let failures = self.consecutive_failures.lock().await;

        match &*state {
            State::Closed => format!("Closed (failures: {})", failures),
            State::Open { opened_at } => {
                format!("Open (opened {:?} ago)", opened_at.elapsed())
            }
            State::HalfOpen { successes } => {
                format!("HalfOpen (successes: {})", successes)
            }
        }
    }

    /// Check if a call is permitted (circuit not open)
    pub async fn is_call_permitted(&self) -> bool {
        let state = self.state.lock().await;
        match &*state {
            State::Open { opened_at } => opened_at.elapsed() >= self.config.timeout,
            _ => true,
        }
    }

    /// Record a successful operation
    pub async fn record_success(&self) {
        self.on_success().await;
    }

    /// Record a failed operation
    pub async fn record_failure(&self) {
        self.on_failure().await;
    }
}

#[derive(Debug)]
pub enum CircuitBreakerError<E> {
    Open,
    Inner(E),
}

impl<E: std::fmt::Display> std::fmt::Display for CircuitBreakerError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitBreakerError::Open => write!(f, "Circuit breaker is open"),
            CircuitBreakerError::Inner(e) => write!(f, "{}", e),
        }
    }
}

impl<E: std::error::Error> std::error::Error for CircuitBreakerError<E> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_opens_after_failures() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_secs(60),
        });

        for _ in 0..3 {
            let _ = cb.call(|| async { Err::<(), _>("fail") }).await;
        }

        let result = cb.call(|| async { Ok::<_, String>(()) }).await;
        assert!(matches!(result, Err(CircuitBreakerError::Open)));
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_recovery() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout: Duration::from_millis(100),
        });

        for _ in 0..2 {
            let _ = cb.call(|| async { Err::<(), _>("fail") }).await;
        }

        tokio::time::sleep(Duration::from_millis(150)).await;

        for _ in 0..2 {
            let _ = cb.call(|| async { Ok::<_, String>(()) }).await;
        }

        let result = cb.call(|| async { Ok::<_, String>(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_circuit_breaker_stays_closed_on_success() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());

        for _ in 0..10 {
            let result = cb.call(|| async { Ok::<_, String>(()) }).await;
            assert!(result.is_ok());
        }
    }
}
