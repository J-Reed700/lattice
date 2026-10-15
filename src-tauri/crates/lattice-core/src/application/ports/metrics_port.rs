use async_trait::async_trait;

pub struct MetricsSnapshotData {
    pub documents_indexed: u64,
    pub searches_performed: u64,
    pub qa_queries: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub avg_search_time_ms: f64,
    pub avg_qa_time_ms: f64,
    pub uptime_seconds: u64,
}

#[async_trait]
pub trait MetricsPort: Send + Sync {
    async fn snapshot(&self) -> MetricsSnapshotData;
}
