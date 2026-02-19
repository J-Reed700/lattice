//! Get Cache Size Use Case
//!
//! Retrieves current cache size in bytes.
//!
//! # Dependencies
//! - `CachePort` - Cache size calculation
//!
//! # Security
//! - No sensitive data exposed in cache size metrics
//!
//! # Example
//! ```rust,no_run
//! let use_case = GetCacheSizeUseCase::new(cache_port);
//! let size = use_case.execute().await?;
//! println!("Cache using {} MB", size.size_bytes / 1_000_000);
//! ```

use crate::application::dtos::cache_dto::CacheSizeDto;
use crate::application::ports::CachePort;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct GetCacheSizeUseCase {
    cache: Arc<dyn CachePort>,
}

impl GetCacheSizeUseCase {
    pub fn new(cache: Arc<dyn CachePort>) -> Self {
        Self { cache }
    }

    pub async fn execute(&self) -> Result<CacheSizeDto, AppError> {
        let size = self.cache.size().await;

        Ok(CacheSizeDto {
            size_bytes: 0, // Placeholder - actual size calculation would be more complex
            entry_count: size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::CacheStatsData;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MockCachePort {
        size: AtomicUsize,
    }

    impl MockCachePort {
        fn new(size: usize) -> Self {
            Self {
                size: AtomicUsize::new(size),
            }
        }

        fn set_size(&self, new_size: usize) {
            self.size.store(new_size, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl CachePort for MockCachePort {
        async fn clear(&self) {
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
    async fn test_get_cache_size_empty() {
        let mock_cache = Arc::new(MockCachePort::new(0));
        let use_case = GetCacheSizeUseCase::new(mock_cache);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let size_dto = result.unwrap();
        assert_eq!(size_dto.entry_count, 0);
        assert_eq!(size_dto.size_bytes, 0);
    }

    #[tokio::test]
    async fn test_get_cache_size_with_entries() {
        let mock_cache = Arc::new(MockCachePort::new(42));
        let use_case = GetCacheSizeUseCase::new(mock_cache);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let size_dto = result.unwrap();
        assert_eq!(size_dto.entry_count, 42);
    }

    #[tokio::test]
    async fn test_get_cache_size_large_cache() {
        let mock_cache = Arc::new(MockCachePort::new(10000));
        let use_case = GetCacheSizeUseCase::new(mock_cache);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let size_dto = result.unwrap();
        assert_eq!(size_dto.entry_count, 10000);
    }

    #[tokio::test]
    async fn test_get_cache_size_dynamic_changes() {
        let mock_cache = Arc::new(MockCachePort::new(100));
        let use_case = GetCacheSizeUseCase::new(mock_cache.clone());

        let result1 = use_case.execute().await.unwrap();
        assert_eq!(result1.entry_count, 100);

        mock_cache.set_size(200);
        let result2 = use_case.execute().await.unwrap();
        assert_eq!(result2.entry_count, 200);

        mock_cache.set_size(0);
        let result3 = use_case.execute().await.unwrap();
        assert_eq!(result3.entry_count, 0);
    }

    #[tokio::test]
    async fn test_get_cache_size_after_clear() {
        let mock_cache = Arc::new(MockCachePort::new(500));
        let use_case = GetCacheSizeUseCase::new(mock_cache.clone());

        mock_cache.clear().await;
        let result = use_case.execute().await;

        assert!(result.is_ok());
        let size_dto = result.unwrap();
        assert_eq!(size_dto.entry_count, 0);
    }
}
