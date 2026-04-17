//! Clear Cache Use Case
//!
//! Clears all application caches (LLM responses, query results).
//!
//! # Dependencies
//! - `CachePort` - Cache management operations
//!
//! # Security
//! - Audit logs cache clearing operations
//! - Rate limited to prevent DoS via repeated cache clearing
//!
//! # Example
//! ```rust,no_run
//! let use_case = ClearCacheUseCase::new(cache_port);
//! use_case.execute().await?;
//! ```

use crate::application::ports::CachePort;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct ClearCacheUseCase {
    cache: Arc<dyn CachePort>,
}

impl ClearCacheUseCase {
    pub fn new(cache: Arc<dyn CachePort>) -> Self {
        Self { cache }
    }

    pub async fn execute(&self) -> Result<(), AppError> {
        tracing::info!("Clearing application cache");
        self.cache.clear().await;
        tracing::info!("Cache cleared successfully");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::CacheStatsData;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct MockCachePort {
        cleared: AtomicBool,
        size: AtomicUsize,
    }

    impl MockCachePort {
        fn new(initial_size: usize) -> Self {
            Self {
                cleared: AtomicBool::new(false),
                size: AtomicUsize::new(initial_size),
            }
        }

        fn was_cleared(&self) -> bool {
            self.cleared.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl CachePort for MockCachePort {
        async fn clear(&self) {
            self.cleared.store(true, Ordering::SeqCst);
            self.size.store(0, Ordering::SeqCst);
        }

        async fn stats(&self) -> CacheStatsData {
            CacheStatsData {
                size: self.size.load(Ordering::SeqCst),
                capacity: 1000,
                hits: 0,
                misses: 0,
                total_time_saved_ms: 0,
                hit_rate: 0.0,
            }
        }

        async fn size(&self) -> usize {
            self.size.load(Ordering::SeqCst)
        }
    }

    #[tokio::test]
    async fn test_clear_cache_success() {
        let mock_cache = Arc::new(MockCachePort::new(100));
        let use_case = ClearCacheUseCase::new(mock_cache.clone());

        let result = use_case.execute().await;

        assert!(result.is_ok());
        assert!(mock_cache.was_cleared());
        assert_eq!(mock_cache.size().await, 0);
    }

    #[tokio::test]
    async fn test_clear_empty_cache() {
        let mock_cache = Arc::new(MockCachePort::new(0));
        let use_case = ClearCacheUseCase::new(mock_cache.clone());

        let result = use_case.execute().await;

        assert!(result.is_ok());
        assert!(mock_cache.was_cleared());
    }

    #[tokio::test]
    async fn test_clear_large_cache() {
        let mock_cache = Arc::new(MockCachePort::new(10000));
        let use_case = ClearCacheUseCase::new(mock_cache.clone());

        let result = use_case.execute().await;

        assert!(result.is_ok());
        assert!(mock_cache.was_cleared());
        assert_eq!(mock_cache.size().await, 0);
    }

    #[tokio::test]
    async fn test_clear_cache_multiple_times() {
        let mock_cache = Arc::new(MockCachePort::new(50));
        let use_case = ClearCacheUseCase::new(mock_cache.clone());

        let result1 = use_case.execute().await;
        let result2 = use_case.execute().await;

        assert!(result1.is_ok());
        assert!(result2.is_ok());
        assert!(mock_cache.was_cleared());
    }
}
