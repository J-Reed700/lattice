use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const DEFAULT_BYTE_BUDGET: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct QueryCacheKey {
    pub query_text: String,
    pub filters: Option<String>,
    pub limit: usize,
    pub search_mode: String,
    pub corpus_generation: u64,
    pub model_identity: Option<String>,
}

impl QueryCacheKey {
    pub fn new(
        query: String,
        filter: Option<HashMap<String, serde_json::Value>>,
        limit: usize,
        search_mode: String,
        corpus_generation: u64,
        model_identity: Option<String>,
    ) -> Self {
        let filters = filter.map(|filter| {
            let canonical: BTreeMap<_, _> = filter
                .into_iter()
                .map(|(key, value)| (key, canonical_value(value)))
                .collect();
            serde_json::to_string(&canonical).unwrap_or_default()
        });
        Self {
            query_text: query,
            filters,
            limit,
            search_mode,
            corpus_generation,
            model_identity,
        }
    }

    fn byte_size(&self) -> usize {
        self.query_text.capacity()
            + self.filters.as_ref().map_or(0, String::capacity)
            + self.search_mode.capacity()
            + std::mem::size_of::<usize>()
            + std::mem::size_of::<u64>()
            + self.model_identity.as_ref().map_or(0, String::capacity)
    }
}

fn canonical_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(object) => {
            let sorted: BTreeMap<_, _> = object
                .into_iter()
                .map(|(key, value)| (key, canonical_value(value)))
                .collect();
            serde_json::Value::Object(sorted.into_iter().collect())
        }
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(canonical_value).collect())
        }
        other => other,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedSearchResult {
    pub results: Vec<crate::features::search::dto::SearchResultDto>,
    pub cached_at: i64,
    pub execution_time_ms: u64,
}

impl CachedSearchResult {
    fn byte_size(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.results.capacity()
                * std::mem::size_of::<crate::features::search::dto::SearchResultDto>()
            + self
                .results
                .iter()
                .map(|result| {
                    let encoded_metadata =
                        serde_json::to_vec(&result.metadata).map_or(0, |metadata| metadata.len());
                    result.content.capacity()
                        + result.title.capacity()
                        + result.id.capacity()
                        + result.document_id.as_ref().map_or(0, String::capacity)
                        + result.path.as_ref().map_or(0, String::capacity)
                        // JSON length is a conservative proxy for nested
                        // Value allocations; the multiplier covers map nodes,
                        // strings, and per-value allocator overhead.
                        + encoded_metadata.saturating_mul(8)
                        + result.metadata.capacity()
                            * std::mem::size_of::<(String, serde_json::Value)>()
                })
                .sum::<usize>()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CacheMetrics {
    pub hits: u64,
    pub misses: u64,
    pub total_time_saved_ms: u64,
    pub hit_rate: f64,
}

struct CacheState {
    entries: LruCache<QueryCacheKey, CacheEntry>,
    bytes: usize,
}

struct CacheEntry {
    result: Arc<CachedSearchResult>,
    weight: usize,
}

pub struct QueryCache {
    cache: Mutex<CacheState>,
    ttl_seconds: i64,
    byte_budget: usize,
    generation: AtomicU64,
    hits: AtomicU64,
    misses: AtomicU64,
    total_time_saved_ms: AtomicU64,
}

impl QueryCache {
    pub fn new(capacity: usize, ttl_seconds: i64) -> Self {
        Self::with_byte_budget(capacity, ttl_seconds, DEFAULT_BYTE_BUDGET)
    }

    fn with_byte_budget(capacity: usize, ttl_seconds: i64, byte_budget: usize) -> Self {
        Self {
            cache: Mutex::new(CacheState {
                entries: LruCache::new(NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::MIN)),
                bytes: 0,
            }),
            ttl_seconds,
            byte_budget,
            generation: AtomicU64::new(0),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            total_time_saved_ms: AtomicU64::new(0),
        }
    }

    /// Capture this before starting a search; publication is rejected if a
    /// corpus mutation invalidates the cache while that search is in flight.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    pub fn get(&self, key: &QueryCacheKey) -> Option<CachedSearchResult> {
        if key.corpus_generation != self.generation() {
            self.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        let cached = {
            let mut state = self.cache.lock().unwrap_or_else(|poisoned| {
                tracing::warn!("Query cache mutex poisoned, recovering");
                poisoned.into_inner()
            });
            state
                .entries
                .get(key)
                .map(|entry| Arc::clone(&entry.result))
        };
        if let Some(cached) = cached {
            if chrono::Utc::now()
                .timestamp()
                .saturating_sub(cached.cached_at)
                < self.ttl_seconds
            {
                self.hits.fetch_add(1, Ordering::Relaxed);
                self.total_time_saved_ms
                    .fetch_add(cached.execution_time_ms, Ordering::Relaxed);
                return Some((*cached).clone());
            }
            let mut state = self
                .cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(expired) = state.entries.pop(key) {
                state.bytes = state.bytes.saturating_sub(expired.weight);
            }
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    pub fn put(&self, key: QueryCacheKey, result: CachedSearchResult) {
        self.put_if_generation(key, result);
    }

    pub fn put_if_generation(&self, key: QueryCacheKey, result: CachedSearchResult) -> bool {
        let generation = self.generation();
        if key.corpus_generation != generation {
            return false;
        }
        let entry_bytes = key.byte_size().saturating_add(result.byte_size());
        if entry_bytes > self.byte_budget {
            return false;
        }
        let mut state = self.cache.lock().unwrap_or_else(|poisoned| {
            tracing::warn!("Query cache mutex poisoned, recovering");
            poisoned.into_inner()
        });
        // Recheck after taking the lock: invalidation increments the epoch
        // before clearing, so an older search cannot publish after that clear.
        if key.corpus_generation != self.generation() {
            return false;
        }
        if let Some(old) = state.entries.pop(&key) {
            state.bytes = state.bytes.saturating_sub(old.weight);
        }
        while state.entries.len() >= state.entries.cap().get()
            || state.bytes.saturating_add(entry_bytes) > self.byte_budget
        {
            let Some((_old_key, old_value)) = state.entries.pop_lru() else {
                break;
            };
            state.bytes = state.bytes.saturating_sub(old_value.weight);
        }
        state.entries.put(
            key,
            CacheEntry {
                result: Arc::new(result),
                weight: entry_bytes,
            },
        );
        state.bytes = state.bytes.saturating_add(entry_bytes);
        true
    }

    pub fn invalidate_corpus(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.clear_entries();
    }

    fn clear_entries(&self) {
        let mut state = self
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.entries.clear();
        state.bytes = 0;
    }

    pub fn clear(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.clear_entries();
        tracing::info!("Query cache cleared");
    }

    pub fn stats(&self) -> CacheStats {
        let state = self
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total_time_saved = self.total_time_saved_ms.load(Ordering::Relaxed);
        let total_queries = hits + misses;
        CacheStats {
            size: state.entries.len(),
            capacity: state.entries.cap().get(),
            hits,
            misses,
            total_time_saved_ms: total_time_saved,
            hit_rate: if total_queries > 0 {
                hits as f64 / total_queries as f64 * 100.0
            } else {
                0.0
            },
        }
    }

    pub fn metrics(&self) -> CacheMetrics {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total_time_saved_ms = self.total_time_saved_ms.load(Ordering::Relaxed);
        let total = hits + misses;
        CacheMetrics {
            hits,
            misses,
            total_time_saved_ms,
            hit_rate: if total > 0 {
                hits as f64 / total as f64
            } else {
                0.0
            },
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

pub static QUERY_CACHE: Lazy<Arc<QueryCache>> = Lazy::new(|| Arc::new(QueryCache::new(1000, 600)));

pub fn invalidate_query_cache() {
    QUERY_CACHE.invalidate_corpus();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn result(content: &str) -> CachedSearchResult {
        CachedSearchResult {
            results: vec![crate::features::search::dto::SearchResultDto {
                id: "chunk".into(),
                title: "title".into(),
                content: content.into(),
                score: 0.9,
                path: None,
                document_id: Some("doc".into()),
                position: None,
                vector_score: None,
                bm25_score: None,
                vector_rank: None,
                bm25_rank: None,
                metadata: HashMap::new(),
            }],
            cached_at: chrono::Utc::now().timestamp(),
            execution_time_ms: 5,
        }
    }

    #[test]
    fn canonicalizes_nested_filter_object_order() {
        let first = QueryCacheKey::new(
            "q".into(),
            Some(HashMap::from([(
                "filter".into(),
                json!({"b": 2, "a": {"y":1,"x":0}}),
            )])),
            5,
            "Vector".into(),
            0,
            None,
        );
        let second = QueryCacheKey::new(
            "q".into(),
            Some(HashMap::from([(
                "filter".into(),
                json!({"a": {"x":0,"y":1}, "b": 2}),
            )])),
            5,
            "Vector".into(),
            0,
            None,
        );
        assert_eq!(first, second);
    }

    #[test]
    fn invalidation_rejects_an_in_flight_publication() {
        let cache = QueryCache::new(8, 600);
        let generation = cache.generation();
        let key = QueryCacheKey::new("q".into(), None, 5, "Vector".into(), generation, None);
        cache.invalidate_corpus();
        assert!(!cache.put_if_generation(key.clone(), result("stale")));
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn evicts_by_entry_and_byte_budget() {
        let cache = QueryCache::with_byte_budget(10, 600, 900);
        let generation = cache.generation();
        let key = |query: &str| {
            QueryCacheKey::new(query.into(), None, 5, "Vector".into(), generation, None)
        };
        assert!(cache.put_if_generation(key("a"), result(&"a".repeat(250))));
        assert!(cache.put_if_generation(key("b"), result(&"b".repeat(250))));
        let stats = cache.stats();
        assert!(
            stats.size <= 1,
            "byte budget should evict old entries: {stats:?}"
        );
        assert!(!cache.put_if_generation(key("huge"), result(&"x".repeat(2_000))));
    }

    #[test]
    fn count_capacity_eviction_keeps_byte_accounting_exact() {
        let cache = QueryCache::with_byte_budget(1, 600, 1_000_000);
        let generation = cache.generation();
        let key = |query: &str| {
            QueryCacheKey::new(query.into(), None, 5, "Vector".into(), generation, None)
        };
        let first_key = key("first");
        let first_result = result("first result");
        let first_weight = first_key.byte_size() + first_result.byte_size();
        assert!(cache.put_if_generation(first_key, first_result));
        assert_eq!(cache.cache.lock().unwrap().bytes, first_weight);

        let second_key = key("second");
        let second_result = result("second result");
        let second_weight = second_key.byte_size() + second_result.byte_size();
        assert!(cache.put_if_generation(second_key, second_result));
        let state = cache.cache.lock().unwrap();
        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.bytes, second_weight);
    }
}
