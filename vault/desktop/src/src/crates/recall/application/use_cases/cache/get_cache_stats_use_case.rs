//! Get Cache Stats Use Case
//!
//! Retrieves detailed cache statistics (hits, misses, evictions).
//!
//! # Dependencies
//! - `CachePort` - Cache statistics tracking
//!
//! # Security
//! - Aggregated metrics only, no individual cache entry exposure
//!
//! # Example
//! ```rust,no_run
//! let use_case = GetCacheStatsUseCase::new(cache_port);
//! let stats = use_case.execute().await?;
//! println!("Cache hit rate: {:.2}%", stats.hit_rate * 100.0);
//! ```

use crate::application::dtos::cache_dto::CacheStatsDto;
use crate::application::ports::CachePort;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct GetCacheStatsUseCase {
    cache: Arc<dyn CachePort>,
}

impl GetCacheStatsUseCase {
    pub fn new(cache: Arc<dyn CachePort>) -> Self {
        Self { cache }
    }

    pub async fn execute(&self) -> Result<CacheStatsDto, AppError> {
        let stats = self.cache.stats().await;

        Ok(CacheStatsDto {
            size: stats.size,
            capacity: stats.capacity,
            hits: stats.hits,
            misses: stats.misses,
            total_time_saved_ms: stats.total_time_saved_ms,
            hit_rate: stats.hit_rate,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::CacheStatsData;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    struct MockCachePort {
        size: AtomicUsize,
        capacity: usize,
        hits: AtomicU64,
        misses: AtomicU64,
        total_time_saved_ms: AtomicU64,
    }

    impl MockCachePort {
        fn new(size: usize, capacity: usize, hits: u64, misses: u64, time_saved: u64) -> Self {
            Self {
                size: AtomicUsize::new(size),
                capacity,
                hits: AtomicU64::new(hits),
                misses: AtomicU64::new(misses),
                total_time_saved_ms: AtomicU64::new(time_saved),
            }
        }

        fn empty() -> Self {
            Self::new(0, 1000, 0, 0, 0)
        }
    }

    #[async_trait]
    impl CachePort for MockCachePort {
        async fn clear(&self) {
            self.size.store(0, Ordering::SeqCst);
        }

        async fn stats(&self) -> CacheStatsData {
            let size = self.size.load(Ordering::SeqCst);
            let hits = self.hits.load(Ordering::SeqCst);
            let misses = self.misses.load(Ordering::SeqCst);
            let total = hits + misses;
            let hit_rate = if total > 0 {
                hits as f64 / total as f64
            } else {
                0.0
            };

            CacheStatsData {
                size,
                capacity: self.capacity,
                hits,
                misses,
                total_time_saved_ms: self.total_time_saved_ms.load(Ordering::SeqCst),
                hit_rate,
            }
        }

        async fn size(&self) -> usize {
            self.size.load(Ordering::SeqCst)
        }
    }

    #[tokio::test]
    async fn test_get_cache_stats_empty() {
        let mock_cache = Arc::new(MockCachePort::empty());
        let use_case = GetCacheStatsUseCase::new(mock_cache);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let stats = result.unwrap();
        assert_eq!(stats.size, 0);
        assert_eq!(stats.capacity, 1000);
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.total_time_saved_ms, 0);
        assert_eq!(stats.hit_rate, 0.0);
    }

    #[tokio::test]
    async fn test_get_cache_stats_with_data() {
        let mock_cache = Arc::new(MockCachePort::new(500, 1000, 80, 20, 1500));
        let use_case = GetCacheStatsUseCase::new(mock_cache);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let stats = result.unwrap();
        assert_eq!(stats.size, 500);
        assert_eq!(stats.capacity, 1000);
        assert_eq!(stats.hits, 80);
        assert_eq!(stats.misses, 20);
        assert_eq!(stats.total_time_saved_ms, 1500);
        assert_eq!(stats.hit_rate, 0.8);
    }

    #[tokio::test]
    async fn test_get_cache_stats_perfect_hit_rate() {
        let mock_cache = Arc::new(MockCachePort::new(100, 200, 100, 0, 5000));
        let use_case = GetCacheStatsUseCase::new(mock_cache);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let stats = result.unwrap();
        assert_eq!(stats.hits, 100);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.hit_rate, 1.0);
    }

    #[tokio::test]
    async fn test_get_cache_stats_zero_hit_rate() {
        let mock_cache = Arc::new(MockCachePort::new(50, 200, 0, 100, 0));
        let use_case = GetCacheStatsUseCase::new(mock_cache);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let stats = result.unwrap();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 100);
        assert_eq!(stats.hit_rate, 0.0);
    }

    #[tokio::test]
    async fn test_get_cache_stats_full_capacity() {
        let mock_cache = Arc::new(MockCachePort::new(1000, 1000, 500, 500, 10000));
        let use_case = GetCacheStatsUseCase::new(mock_cache);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let stats = result.unwrap();
        assert_eq!(stats.size, stats.capacity);
        assert_eq!(stats.hit_rate, 0.5);
    }

    #[tokio::test]
    async fn test_get_cache_stats_large_numbers() {
        let mock_cache = Arc::new(MockCachePort::new(5000, 10000, 1_000_000, 100_000, 500_000));
        let use_case = GetCacheStatsUseCase::new(mock_cache);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let stats = result.unwrap();
        assert_eq!(stats.hits, 1_000_000);
        assert_eq!(stats.misses, 100_000);
        assert!(stats.hit_rate > 0.9);
    }
}
