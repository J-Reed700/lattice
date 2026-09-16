use lru::LruCache;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

const DEFAULT_CACHE_SIZE: usize = 500;
const TAG_CACHE_SIZE: usize = 1000;

#[derive(Clone)]
pub struct LlmCache {
    tag_cache: Arc<RwLock<LruCache<u64, Vec<String>>>>,
    query_cache: Arc<RwLock<LruCache<u64, String>>>,
    tag_hits: Arc<AtomicU64>,
    tag_misses: Arc<AtomicU64>,
    query_hits: Arc<AtomicU64>,
    query_misses: Arc<AtomicU64>,
}

impl LlmCache {
    pub fn new() -> Self {
        Self {
            tag_cache: Arc::new(RwLock::new(LruCache::new(
                NonZeroUsize::new(TAG_CACHE_SIZE).unwrap_or(NonZeroUsize::MIN),
            ))),
            query_cache: Arc::new(RwLock::new(LruCache::new(
                NonZeroUsize::new(DEFAULT_CACHE_SIZE).unwrap_or(NonZeroUsize::MIN),
            ))),
            tag_hits: Arc::new(AtomicU64::new(0)),
            tag_misses: Arc::new(AtomicU64::new(0)),
            query_hits: Arc::new(AtomicU64::new(0)),
            query_misses: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn with_capacity(tag_capacity: usize, query_capacity: usize) -> Self {
        Self {
            tag_cache: Arc::new(RwLock::new(LruCache::new(
                NonZeroUsize::new(tag_capacity).unwrap_or(NonZeroUsize::MIN),
            ))),
            query_cache: Arc::new(RwLock::new(LruCache::new(
                NonZeroUsize::new(query_capacity).unwrap_or(NonZeroUsize::MIN),
            ))),
            tag_hits: Arc::new(AtomicU64::new(0)),
            tag_misses: Arc::new(AtomicU64::new(0)),
            query_hits: Arc::new(AtomicU64::new(0)),
            query_misses: Arc::new(AtomicU64::new(0)),
        }
    }

    fn hash_content(content: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        hasher.finish()
    }

    pub async fn get_tags(&self, content: &str) -> Option<Vec<String>> {
        let key = Self::hash_content(content);
        let mut cache = self.tag_cache.write().await;
        let result = cache.get(&key).cloned();
        if result.is_some() {
            self.tag_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.tag_misses.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    pub async fn put_tags(&self, content: &str, tags: Vec<String>) {
        let key = Self::hash_content(content);
        let mut cache = self.tag_cache.write().await;
        cache.put(key, tags);
    }

    pub async fn get_query_rewrite(&self, query: &str) -> Option<String> {
        let key = Self::hash_content(query);
        let mut cache = self.query_cache.write().await;
        let result = cache.get(&key).cloned();
        if result.is_some() {
            self.query_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.query_misses.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    pub async fn put_query_rewrite(&self, query: &str, rewritten: String) {
        let key = Self::hash_content(query);
        let mut cache = self.query_cache.write().await;
        cache.put(key, rewritten);
    }

    pub async fn clear_tag_cache(&self) {
        let mut cache = self.tag_cache.write().await;
        cache.clear();
    }

    pub async fn clear_query_cache(&self) {
        let mut cache = self.query_cache.write().await;
        cache.clear();
    }

    pub async fn clear_all(&self) {
        self.clear_tag_cache().await;
        self.clear_query_cache().await;
    }

    pub async fn stats(&self) -> CacheStats {
        let tag_cache = self.tag_cache.read().await;
        let query_cache = self.query_cache.read().await;

        CacheStats {
            tag_cache_size: tag_cache.len(),
            tag_cache_capacity: tag_cache.cap().get(),
            query_cache_size: query_cache.len(),
            query_cache_capacity: query_cache.cap().get(),
            tag_hits: self.tag_hits.load(Ordering::Relaxed),
            tag_misses: self.tag_misses.load(Ordering::Relaxed),
            query_hits: self.query_hits.load(Ordering::Relaxed),
            query_misses: self.query_misses.load(Ordering::Relaxed),
        }
    }
}

impl Default for LlmCache {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheStats {
    pub tag_cache_size: usize,
    pub tag_cache_capacity: usize,
    pub query_cache_size: usize,
    pub query_cache_capacity: usize,
    pub tag_hits: u64,
    pub tag_misses: u64,
    pub query_hits: u64,
    pub query_misses: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tag_cache() {
        let cache = LlmCache::new();

        assert!(cache.get_tags("test content").await.is_none());

        let tags = vec!["rust".to_string(), "programming".to_string()];
        cache.put_tags("test content", tags.clone()).await;

        let cached = cache.get_tags("test content").await;
        assert_eq!(cached, Some(tags));
    }

    #[tokio::test]
    async fn test_query_cache() {
        let cache = LlmCache::new();

        assert!(cache.get_query_rewrite("original query").await.is_none());

        cache
            .put_query_rewrite("original query", "rewritten query".to_string())
            .await;

        let cached = cache.get_query_rewrite("original query").await;
        assert_eq!(cached, Some("rewritten query".to_string()));
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let cache = LlmCache::new();

        cache.put_tags("content", vec!["tag".to_string()]).await;
        cache
            .put_query_rewrite("query", "rewrite".to_string())
            .await;

        cache.clear_all().await;

        assert!(cache.get_tags("content").await.is_none());
        assert!(cache.get_query_rewrite("query").await.is_none());
    }

    #[tokio::test]
    async fn test_lru_eviction() {
        let cache = LlmCache::with_capacity(2, 2);

        cache.put_tags("content1", vec!["tag1".to_string()]).await;
        cache.put_tags("content2", vec!["tag2".to_string()]).await;
        cache.put_tags("content3", vec!["tag3".to_string()]).await;

        assert!(cache.get_tags("content1").await.is_none());
        assert!(cache.get_tags("content2").await.is_some());
        assert!(cache.get_tags("content3").await.is_some());
    }

    #[tokio::test]
    async fn test_stats() {
        let cache = LlmCache::new();

        let stats = cache.stats().await;
        assert_eq!(stats.tag_cache_size, 0);
        assert_eq!(stats.tag_cache_capacity, TAG_CACHE_SIZE);

        cache.put_tags("content", vec!["tag".to_string()]).await;

        let stats = cache.stats().await;
        assert_eq!(stats.tag_cache_size, 1);
    }

    #[tokio::test]
    async fn test_concurrent_get_put_operations() {
        let cache = Arc::new(LlmCache::with_capacity(100, 100));

        let mut handles = vec![];

        for thread_id in 0..10 {
            let cache_clone = Arc::clone(&cache);

            let handle = tokio::spawn(async move {
                for op_id in 0..100 {
                    let content = format!("thread_{}_op_{}", thread_id, op_id);
                    let tags = vec![
                        format!("tag_{}_{}_1", thread_id, op_id),
                        format!("tag_{}_{}_2", thread_id, op_id),
                    ];

                    // Put operation
                    cache_clone.put_tags(&content, tags.clone()).await;

                    let retrieved = cache_clone.get_tags(&content).await;

                    if let Some(retrieved_tags) = retrieved {
                        assert_eq!(
                            retrieved_tags, tags,
                            "CRITICAL CONCURRENCY: Data corruption detected in thread {}",
                            thread_id
                        );
                    }
                }
            });

            handles.push(handle);
        }

        for handle in handles {
            handle
                .await
                .expect("CRITICAL CONCURRENCY: Thread panicked during concurrent operations");
        }

        let stats = cache.stats().await;
        assert!(
            stats.tag_cache_size <= stats.tag_cache_capacity,
            "CRITICAL CONCURRENCY: Cache size {} exceeds capacity {}",
            stats.tag_cache_size,
            stats.tag_cache_capacity
        );
    }

    #[tokio::test]
    async fn test_concurrent_readers_writers() {
        let cache = Arc::new(LlmCache::with_capacity(50, 50));

        // Pre-populate cache
        for i in 0..10 {
            let content = format!("content_{}", i);
            let tags = vec![format!("tag_{}", i)];
            cache.put_tags(&content, tags).await;
        }

        let mut handles = vec![];

        // Spawn 5 reader threads
        for _reader_id in 0..5 {
            let cache_clone = Arc::clone(&cache);

            let handle = tokio::spawn(async move {
                for _ in 0..50 {
                    for i in 0..10 {
                        let content = format!("content_{}", i);
                        let _ = cache_clone.get_tags(&content).await;
                    }

                    // Small yield to allow writers
                    tokio::task::yield_now().await;
                }
            });

            handles.push(handle);
        }

        // Spawn 5 writer threads
        for writer_id in 0..5 {
            let cache_clone = Arc::clone(&cache);

            let handle = tokio::spawn(async move {
                for op in 0..50 {
                    let content = format!("writer_{}_{}", writer_id, op);
                    let tags = vec![format!("tag_{}_{}", writer_id, op)];
                    cache_clone.put_tags(&content, tags).await;

                    // Small yield to allow readers
                    tokio::task::yield_now().await;
                }
            });

            handles.push(handle);
        }

        for handle in handles {
            handle
                .await
                .expect("CRITICAL CONCURRENCY: Thread panicked or deadlocked");
        }

        let stats = cache.stats().await;
        assert!(
            stats.tag_cache_size <= stats.tag_cache_capacity,
            "CRITICAL CONCURRENCY: Cache capacity violated under concurrent load"
        );
    }

    #[tokio::test]
    async fn test_concurrent_eviction_under_load() {
        let capacity = 20;
        let cache = Arc::new(LlmCache::with_capacity(capacity, capacity));

        let mut handles = vec![];

        for thread_id in 0..5 {
            let cache_clone = Arc::clone(&cache);

            let handle = tokio::spawn(async move {
                for item_id in 0..50 {
                    let content = format!("thread_{}_item_{}", thread_id, item_id);
                    let tags = vec![format!("tag_{}_{}", thread_id, item_id)];

                    cache_clone.put_tags(&content, tags).await;

                    // Yield occasionally to interleave operations
                    if item_id % 10 == 0 {
                        tokio::task::yield_now().await;
                    }
                }
            });

            handles.push(handle);
        }

        for handle in handles {
            handle
                .await
                .expect("CRITICAL CONCURRENCY: Thread panicked during eviction");
        }

        let stats = cache.stats().await;

        assert!(
            stats.tag_cache_size <= capacity,
            "CRITICAL CONCURRENCY: LRU eviction failed - cache has {} items, capacity is {}",
            stats.tag_cache_size,
            capacity
        );

        assert_eq!(
            stats.tag_cache_capacity, capacity,
            "CRITICAL CONCURRENCY: Cache capacity changed unexpectedly"
        );
    }

    #[tokio::test]
    async fn test_concurrent_query_and_tag_cache() {
        let cache = Arc::new(LlmCache::with_capacity(50, 50));

        let mut handles = vec![];

        // Spawn threads for tag cache
        for thread_id in 0..3 {
            let cache_clone = Arc::clone(&cache);

            let handle = tokio::spawn(async move {
                for op_id in 0..30 {
                    let content = format!("tag_thread_{}_op_{}", thread_id, op_id);
                    let tags = vec![format!("tag_{}", op_id)];

                    cache_clone.put_tags(&content, tags.clone()).await;
                    let retrieved = cache_clone.get_tags(&content).await;

                    if let Some(retrieved_tags) = retrieved {
                        assert_eq!(retrieved_tags, tags);
                    }
                }
            });

            handles.push(handle);
        }

        // Spawn threads for query cache (same time as tag cache)
        for thread_id in 0..3 {
            let cache_clone = Arc::clone(&cache);

            let handle = tokio::spawn(async move {
                for op_id in 0..30 {
                    let query = format!("query_thread_{}_op_{}", thread_id, op_id);
                    let rewrite = format!("rewrite_{}_{}", thread_id, op_id);

                    cache_clone.put_query_rewrite(&query, rewrite.clone()).await;
                    let retrieved = cache_clone.get_query_rewrite(&query).await;

                    if let Some(retrieved_rewrite) = retrieved {
                        assert_eq!(retrieved_rewrite, rewrite);
                    }
                }
            });

            handles.push(handle);
        }

        for handle in handles {
            handle
                .await
                .expect("CRITICAL CONCURRENCY: Cache isolation failed");
        }

        let stats = cache.stats().await;

        assert!(
            stats.tag_cache_size > 0,
            "Tag cache should have entries after concurrent operations"
        );

        assert!(
            stats.query_cache_size > 0,
            "Query cache should have entries after concurrent operations"
        );
    }

    #[tokio::test]
    async fn test_capacity_edge_cases() {
        let cache_min = LlmCache::with_capacity(1, 1);

        cache_min.put_tags("first", vec!["tag1".to_string()]).await;
        assert!(cache_min.get_tags("first").await.is_some());

        cache_min.put_tags("second", vec!["tag2".to_string()]).await;
        assert!(
            cache_min.get_tags("first").await.is_none(),
            "First item should be evicted with capacity=1"
        );
        assert!(cache_min.get_tags("second").await.is_some());

        let cache_large = LlmCache::with_capacity(10000, 10000);

        for i in 0..100 {
            cache_large
                .put_tags(&format!("content_{}", i), vec![format!("tag_{}", i)])
                .await;
        }

        let stats = cache_large.stats().await;
        assert_eq!(stats.tag_cache_size, 100);
        assert_eq!(stats.tag_cache_capacity, 10000);

        // All items should still be in cache
        for i in 0..100 {
            assert!(
                cache_large
                    .get_tags(&format!("content_{}", i))
                    .await
                    .is_some(),
                "All items should remain in large capacity cache"
            );
        }
    }
}
