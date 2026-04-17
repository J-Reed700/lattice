//! Cache Adapter Implementation
//!
//! Implements CachePort by wrapping LLM cache.
//!
//! # Features
//! - Clear all caches (LLM tag cache and query cache)
//! - Get cache statistics
//! - Get cache size

use crate::application::ports::cache_port::{CachePort, CacheStatsData};
use crate::features::cache::llm_cache::LlmCache;
use async_trait::async_trait;

/// Cache adapter wrapping LlmCache
#[derive(Clone)]
pub struct CacheAdapter {
    llm_cache: LlmCache,
}

impl CacheAdapter {
    pub fn new(llm_cache: LlmCache) -> Self {
        Self { llm_cache }
    }
}

#[async_trait]
impl CachePort for CacheAdapter {
    async fn clear(&self) {
        self.llm_cache.clear_all().await;
    }

    async fn stats(&self) -> CacheStatsData {
        let llm_stats = self.llm_cache.stats().await;

        // Calculate aggregate stats from LLM cache
        let total_size = llm_stats.tag_cache_size + llm_stats.query_cache_size;
        let total_capacity = llm_stats.tag_cache_capacity + llm_stats.query_cache_capacity;

        let hits = llm_stats.tag_hits + llm_stats.query_hits;
        let misses = llm_stats.tag_misses + llm_stats.query_misses;
        let total_requests = hits + misses;
        let hit_rate = if total_requests > 0 {
            hits as f64 / total_requests as f64
        } else {
            0.0
        };

        CacheStatsData {
            size: total_size,
            capacity: total_capacity,
            hits,
            misses,
            total_time_saved_ms: 0,
            hit_rate,
        }
    }

    async fn size(&self) -> usize {
        let stats = self.llm_cache.stats().await;
        stats.tag_cache_size + stats.query_cache_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_clear_cache() {
        let llm_cache = LlmCache::new();
        let adapter = CacheAdapter::new(llm_cache.clone());

        // Add some items
        llm_cache
            .put_tags("content", vec!["tag1".to_string()])
            .await;
        llm_cache
            .put_query_rewrite("query", "rewrite".to_string())
            .await;

        // Verify they exist
        assert!(llm_cache.get_tags("content").await.is_some());
        assert!(llm_cache.get_query_rewrite("query").await.is_some());

        // Clear
        adapter.clear().await;

        // Verify they're gone
        assert!(llm_cache.get_tags("content").await.is_none());
        assert!(llm_cache.get_query_rewrite("query").await.is_none());
    }

    #[tokio::test]
    async fn test_cache_size() {
        let llm_cache = LlmCache::new();
        let adapter = CacheAdapter::new(llm_cache.clone());

        assert_eq!(adapter.size().await, 0);

        llm_cache.put_tags("content", vec!["tag".to_string()]).await;
        assert_eq!(adapter.size().await, 1);

        llm_cache
            .put_query_rewrite("query", "rewrite".to_string())
            .await;
        assert_eq!(adapter.size().await, 2);
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let llm_cache = LlmCache::new();
        let adapter = CacheAdapter::new(llm_cache.clone());

        let stats = adapter.stats().await;
        assert_eq!(stats.size, 0);
        assert!(stats.capacity > 0);

        llm_cache.put_tags("content", vec!["tag".to_string()]).await;

        let stats = adapter.stats().await;
        assert_eq!(stats.size, 1);
    }
}
