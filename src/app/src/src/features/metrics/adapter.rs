//! Metrics Adapter Implementation
//!
//! Implements MetricsPort by wrapping the existing Metrics service.
//!
//! # Features
//! - Aggregate metrics snapshot
//! - Performance tracking
//! - Cache statistics
//! - Error tracking

use crate::application::ports::metrics_port::{MetricsPort, MetricsSnapshotData};
use crate::infrastructure::observability::metrics::Metrics;
use async_trait::async_trait;

/// Metrics adapter wrapping the Metrics service
#[derive(Clone)]
pub struct MetricsAdapter {
    metrics: Metrics,
}

impl MetricsAdapter {
    pub fn new(metrics: Metrics) -> Self {
        Self { metrics }
    }
}

#[async_trait]
impl MetricsPort for MetricsAdapter {
    async fn snapshot(&self) -> MetricsSnapshotData {
        let snapshot = self.metrics.snapshot();

        MetricsSnapshotData {
            documents_indexed: snapshot.files_indexed,
            searches_performed: snapshot.search_count,
            qa_queries: snapshot.llm_requests,
            cache_hits: snapshot.cache_hits,
            cache_misses: snapshot.cache_misses,
            avg_search_time_ms: snapshot.search_avg_duration_ms,
            avg_qa_time_ms: snapshot.llm_avg_duration_ms,
            uptime_seconds: snapshot.uptime_seconds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_snapshot() {
        let metrics = Metrics::new();
        let adapter = MetricsAdapter::new(metrics.clone());

        let snapshot = adapter.snapshot().await;

        assert_eq!(snapshot.documents_indexed, 0);
        assert_eq!(snapshot.searches_performed, 0);
        assert_eq!(snapshot.cache_hits, 0);
    }

    #[tokio::test]
    async fn test_metrics_after_activity() {
        let metrics = Metrics::new();
        let adapter = MetricsAdapter::new(metrics.clone());

        // Simulate some activity
        metrics.record_search(100);
        metrics.record_cache_hit();
        metrics.record_file_indexed();

        let snapshot = adapter.snapshot().await;

        assert_eq!(snapshot.searches_performed, 1);
        assert_eq!(snapshot.cache_hits, 1);
        assert_eq!(snapshot.documents_indexed, 1);
        assert_eq!(snapshot.avg_search_time_ms, 100.0);
    }

    #[tokio::test]
    async fn test_metrics_averages() {
        let metrics = Metrics::new();
        let adapter = MetricsAdapter::new(metrics.clone());

        // Record multiple searches
        metrics.record_search(100);
        metrics.record_search(200);
        metrics.record_search(300);

        let snapshot = adapter.snapshot().await;

        assert_eq!(snapshot.searches_performed, 3);
        assert_eq!(snapshot.avg_search_time_ms, 200.0);
    }

    #[tokio::test]
    async fn test_llm_metrics() {
        let metrics = Metrics::new();
        let adapter = MetricsAdapter::new(metrics.clone());

        metrics.record_llm_request(500);
        metrics.record_llm_request(1000);

        let snapshot = adapter.snapshot().await;

        assert_eq!(snapshot.qa_queries, 2);
        assert_eq!(snapshot.avg_qa_time_ms, 750.0);
    }

    #[tokio::test]
    async fn test_cache_metrics() {
        let metrics = Metrics::new();
        let adapter = MetricsAdapter::new(metrics.clone());

        metrics.record_cache_hit();
        metrics.record_cache_hit();
        metrics.record_cache_miss();

        let snapshot = adapter.snapshot().await;

        assert_eq!(snapshot.cache_hits, 2);
        assert_eq!(snapshot.cache_misses, 1);
    }
}
