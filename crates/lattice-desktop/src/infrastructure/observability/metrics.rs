use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone)]
pub struct Metrics {
    start_time: Instant,
    search_count: Arc<AtomicU64>,
    search_duration_total_ms: Arc<AtomicU64>,
    llm_requests: Arc<AtomicU64>,
    llm_duration_total_ms: Arc<AtomicU64>,
    cache_hits: Arc<AtomicU64>,
    cache_misses: Arc<AtomicU64>,
    embeddings_generated: Arc<AtomicU64>,
    indexing_operations: Arc<AtomicU64>,
    indexing_errors: Arc<AtomicU64>,
    search_errors: Arc<AtomicU64>,
    llm_errors: Arc<AtomicU64>,
    files_indexed: Arc<AtomicU64>,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            search_count: Arc::new(AtomicU64::new(0)),
            search_duration_total_ms: Arc::new(AtomicU64::new(0)),
            llm_requests: Arc::new(AtomicU64::new(0)),
            llm_duration_total_ms: Arc::new(AtomicU64::new(0)),
            cache_hits: Arc::new(AtomicU64::new(0)),
            cache_misses: Arc::new(AtomicU64::new(0)),
            embeddings_generated: Arc::new(AtomicU64::new(0)),
            indexing_operations: Arc::new(AtomicU64::new(0)),
            indexing_errors: Arc::new(AtomicU64::new(0)),
            search_errors: Arc::new(AtomicU64::new(0)),
            llm_errors: Arc::new(AtomicU64::new(0)),
            files_indexed: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn record_search(&self, duration_ms: u64) {
        self.search_count.fetch_add(1, Ordering::Relaxed);
        self.search_duration_total_ms
            .fetch_add(duration_ms, Ordering::Relaxed);
        tracing::info!(
            duration_ms = duration_ms,
            total_searches = self.search_count.load(Ordering::Relaxed),
            "Search completed"
        );
    }

    pub fn record_search_error(&self) {
        self.search_errors.fetch_add(1, Ordering::Relaxed);
        tracing::error!("Search operation failed");
    }

    pub fn record_llm_request(&self, duration_ms: u64) {
        self.llm_requests.fetch_add(1, Ordering::Relaxed);
        self.llm_duration_total_ms
            .fetch_add(duration_ms, Ordering::Relaxed);
        tracing::info!(
            duration_ms = duration_ms,
            total_requests = self.llm_requests.load(Ordering::Relaxed),
            "LLM request completed"
        );
    }

    pub fn record_llm_error(&self) {
        self.llm_errors.fetch_add(1, Ordering::Relaxed);
        tracing::error!("LLM operation failed");
    }

    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(hit_rate = self.cache_hit_rate(), "Cache hit");
    }

    pub fn record_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(hit_rate = self.cache_hit_rate(), "Cache miss");
    }

    pub fn record_embedding_generated(&self, count: u64) {
        self.embeddings_generated
            .fetch_add(count, Ordering::Relaxed);
        tracing::debug!(
            count = count,
            total = self.embeddings_generated.load(Ordering::Relaxed),
            "Embeddings generated"
        );
    }

    pub fn record_indexing_operation(&self) {
        self.indexing_operations.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_indexing_error(&self) {
        self.indexing_errors.fetch_add(1, Ordering::Relaxed);
        tracing::error!("Indexing operation failed");
    }

    pub fn record_file_indexed(&self) {
        self.files_indexed.fetch_add(1, Ordering::Relaxed);
        tracing::info!(
            total_files = self.files_indexed.load(Ordering::Relaxed),
            "File indexed"
        );
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            uptime_seconds: self.start_time.elapsed().as_secs(),
            search_count: self.search_count.load(Ordering::Relaxed),
            search_avg_duration_ms: self.avg_search_duration(),
            search_error_count: self.search_errors.load(Ordering::Relaxed),
            llm_requests: self.llm_requests.load(Ordering::Relaxed),
            llm_avg_duration_ms: self.avg_llm_duration(),
            llm_error_count: self.llm_errors.load(Ordering::Relaxed),
            cache_hit_rate: self.cache_hit_rate(),
            cache_hits: self.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.cache_misses.load(Ordering::Relaxed),
            embeddings_generated: self.embeddings_generated.load(Ordering::Relaxed),
            indexing_operations: self.indexing_operations.load(Ordering::Relaxed),
            indexing_errors: self.indexing_errors.load(Ordering::Relaxed),
            files_indexed: self.files_indexed.load(Ordering::Relaxed),
        }
    }

    fn avg_search_duration(&self) -> f64 {
        let count = self.search_count.load(Ordering::Relaxed);
        if count == 0 {
            return 0.0;
        }

        let total = self.search_duration_total_ms.load(Ordering::Relaxed);
        total as f64 / count as f64
    }

    fn avg_llm_duration(&self) -> f64 {
        let count = self.llm_requests.load(Ordering::Relaxed);
        if count == 0 {
            return 0.0;
        }

        let total = self.llm_duration_total_ms.load(Ordering::Relaxed);
        total as f64 / count as f64
    }

    fn cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;

        if total == 0 {
            return 0.0;
        }
        hits as f64 / total as f64 * 100.0
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MetricsSnapshot {
    pub uptime_seconds: u64,
    pub search_count: u64,
    pub search_avg_duration_ms: f64,
    pub search_error_count: u64,
    pub llm_requests: u64,
    pub llm_avg_duration_ms: f64,
    pub llm_error_count: u64,
    pub cache_hit_rate: f64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub embeddings_generated: u64,
    pub indexing_operations: u64,
    pub indexing_errors: u64,
    pub files_indexed: u64,
}
