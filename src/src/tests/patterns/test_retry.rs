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

mod retry_tests {
    use std::time::Duration;
    use lattice::patterns::{retry, retry_with_backoff, RetryConfig};

    #[tokio::test]
    async fn test_retry_succeeds_after_failures() {
        let mut attempt = 0;
        let result = retry(|| async {
            attempt += 1;
            if attempt < 3 {
                Err("fail")
            } else {
                Ok("success")
            }
        })
        .await;

        assert_eq!(result, Ok("success"));
        assert_eq!(attempt, 3);
    }

    #[tokio::test]
    async fn test_retry_fails_after_max_attempts() {
        let mut attempt = 0;
        let result = retry_with_backoff(
            || async {
                attempt += 1;
                Err::<(), _>("fail")
            },
            RetryConfig {
                max_attempts: 2,
                initial_delay: Duration::from_millis(10),
                max_delay: Duration::from_millis(100),
                multiplier: 2.0,
            },
        )
        .await;

        assert!(result.is_err());
        assert_eq!(attempt, 2);
    }

    #[tokio::test]
    async fn test_retry_succeeds_immediately() {
        let mut attempt = 0;
        let result = retry(|| async {
            attempt += 1;
            Ok::<_, String>("success")
        })
        .await;

        assert_eq!(result, Ok("success"));
        assert_eq!(attempt, 1);
    }

    #[tokio::test]
    async fn test_exponential_backoff() {
        let start = std::time::Instant::now();
        let mut attempt = 0;

        let _ = retry_with_backoff(
            || async {
                attempt += 1;
                if attempt < 3 {
                    Err("fail")
                } else {
                    Ok(())
                }
            },
            RetryConfig {
                max_attempts: 3,
                initial_delay: Duration::from_millis(50),
                max_delay: Duration::from_secs(1),
                multiplier: 2.0,
            },
        )
        .await;

        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(150));
    }

    #[tokio::test]
    async fn test_retry_with_custom_config() {
        let mut attempt = 0;
        let result = retry_with_backoff(
            || async {
                attempt += 1;
                if attempt < 5 {
                    Err("fail")
                } else {
                    Ok("success")
                }
            },
            RetryConfig {
                max_attempts: 5,
                initial_delay: Duration::from_millis(10),
                max_delay: Duration::from_millis(100),
                multiplier: 1.5,
            },
        )
        .await;

        assert_eq!(result, Ok("success"));
        assert_eq!(attempt, 5);
    }
}
