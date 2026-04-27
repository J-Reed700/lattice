#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]


#[cfg(test)]
// Test code - allow common test patterns

mod circuit_breaker_tests {
    use std::time::Duration;
    use lattice::patterns::{CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError};

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

    #[tokio::test]
    async fn test_circuit_breaker_timeout_transitions_to_half_open() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 1,
            timeout: Duration::from_millis(50),
        });

        for _ in 0..2 {
            let _ = cb.call(|| async { Err::<(), _>("fail") }).await;
        }

        let result = cb.call(|| async { Ok::<_, String>(()) }).await;
        assert!(matches!(result, Err(CircuitBreakerError::Open)));

        tokio::time::sleep(Duration::from_millis(100)).await;

        let result = cb.call(|| async { Ok::<_, String>("success") }).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_circuit_breaker_state_info() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());

        let state = cb.get_state_info().await;
        assert!(state.contains("Closed"));

        for _ in 0..5 {
            let _ = cb.call(|| async { Err::<(), _>("fail") }).await;
        }

        let state = cb.get_state_info().await;
        assert!(state.contains("Open"));
    }
}
