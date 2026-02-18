use async_trait::async_trait;

pub struct CacheStatsData {
    pub size: usize,
    pub capacity: usize,
    pub hits: u64,
    pub misses: u64,
    pub total_time_saved_ms: u64,
    pub hit_rate: f64,
}

#[async_trait]
pub trait CachePort: Send + Sync {
    async fn clear(&self);
    async fn stats(&self) -> CacheStatsData;
    async fn size(&self) -> usize;
}
