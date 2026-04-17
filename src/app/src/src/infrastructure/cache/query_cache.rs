use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct QueryCacheKey {
    pub query_text: String,
    pub filters: Option<String>,
    pub limit: usize,
    pub search_mode: String,
}

impl QueryCacheKey {
    pub fn new(
        query: String,
        filter: Option<HashMap<String, serde_json::Value>>,
        limit: usize,
        search_mode: String,
    ) -> Self {
        let filters = filter.map(|f| serde_json::to_string(&f).unwrap_or_default());
        Self {
            query_text: query,
            filters,
            limit,
            search_mode,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedSearchResult {
    pub results: Vec<crate::application::dtos::search_dto::SearchResultDto>,
    pub cached_at: i64,
    pub execution_time_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CacheMetrics {
    pub hits: u64,
    pub misses: u64,
    pub total_time_saved_ms: u64,
    pub hit_rate: f64,
}

pub struct QueryCache {
    cache: Mutex<LruCache<QueryCacheKey, CachedSearchResult>>,
    ttl_seconds: i64,
    hits: AtomicU64,
    misses: AtomicU64,
    total_time_saved_ms: AtomicU64,
}

impl QueryCache {
    pub fn new(capacity: usize, ttl_seconds: i64) -> Self {
        Self {
            cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::MIN),
            )),
            ttl_seconds,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            total_time_saved_ms: AtomicU64::new(0),
        }
    }

    pub fn get(&self, key: &QueryCacheKey) -> Option<CachedSearchResult> {
        let mut cache = self.cache.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("Query cache mutex poisoned, recovering");
            poisoned.into_inner()
        });

        if let Some(cached) = cache.get(key) {
            let now = chrono::Utc::now().timestamp();

            if (now - cached.cached_at) < self.ttl_seconds {
                self.hits.fetch_add(1, Ordering::Relaxed);
                self.total_time_saved_ms
                    .fetch_add(cached.execution_time_ms, Ordering::Relaxed);
                return Some(cached.clone());
            } else {
                cache.pop(key);
            }
        }

        self.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    pub fn put(&self, key: QueryCacheKey, result: CachedSearchResult) {
        let mut cache = self.cache.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("Query cache mutex poisoned, recovering");
            poisoned.into_inner()
        });
        cache.put(key, result);
    }

    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("Query cache mutex poisoned, recovering");
            poisoned.into_inner()
        });
        cache.clear();
        tracing::info!("Query cache cleared");
    }

    pub fn stats(&self) -> CacheStats {
        let cache = self.cache.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("Query cache mutex poisoned, recovering");
            poisoned.into_inner()
        });
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total_time_saved = self.total_time_saved_ms.load(Ordering::Relaxed);

        let total_queries = hits + misses;
        let hit_rate = if total_queries > 0 {
            (hits as f64 / total_queries as f64) * 100.0
        } else {
            0.0
        };

        CacheStats {
            size: cache.len(),
            capacity: cache.cap().get(),
            hits,
            misses,
            total_time_saved_ms: total_time_saved,
            hit_rate,
        }
    }

    pub fn metrics(&self) -> CacheMetrics {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total_time_saved = self.total_time_saved_ms.load(Ordering::Relaxed);

        let total_queries = hits + misses;
        let hit_rate = if total_queries > 0 {
            hits as f64 / total_queries as f64
        } else {
            0.0
        };

        CacheMetrics {
            hits,
            misses,
            total_time_saved_ms: total_time_saved,
            hit_rate,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct CacheStats {
    pub size: usize,
    pub capacity: usize,
    pub hits: u64,
    pub misses: u64,
    pub total_time_saved_ms: u64,
    pub hit_rate: f64,
}

use once_cell::sync::Lazy;

pub static QUERY_CACHE: Lazy<Arc<QueryCache>> = Lazy::new(|| {
    Arc::new(QueryCache::new(
        1000, 600, // 10 minutes for 10-20% more hits
    ))
});
