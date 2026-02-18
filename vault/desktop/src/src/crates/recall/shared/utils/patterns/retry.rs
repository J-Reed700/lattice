use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            multiplier: 2.0,
        }
    }
}

pub async fn retry_with_backoff<T, E, F, Fut>(mut operation: F, config: RetryConfig) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 0;
    let mut delay = config.initial_delay;

    loop {
        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    tracing::info!("Operation succeeded after {} retries", attempt);
                }
                return Ok(result);
            }
            Err(e) => {
                attempt += 1;
                if attempt >= config.max_attempts {
                    tracing::error!(
                        "Operation failed after {} attempts: {}",
                        config.max_attempts,
                        e
                    );
                    return Err(e);
                }

                tracing::warn!("Attempt {} failed: {}, retrying in {:?}", attempt, e, delay);
                sleep(delay).await;

                delay = Duration::from_millis(
                    (delay.as_millis() as f64 * config.multiplier)
                        .min(config.max_delay.as_millis() as f64) as u64,
                );
            }
        }
    }
}

pub async fn retry<T, E, F, Fut>(operation: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    retry_with_backoff(operation, RetryConfig::default()).await
}
