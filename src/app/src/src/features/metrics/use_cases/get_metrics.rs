//! Get Metrics Use Case
//!
//! Retrieves application performance and usage metrics snapshot.
//!
//! # Dependencies
//! - `MetricsPort` - Metrics collection and aggregation
//!
//! # Security
//! - Aggregated metrics only, no user-identifiable information
//! - Rate limited to prevent metric enumeration
//!
//! # Example
//! ```rust,no_run
//! let use_case = GetMetricsUseCase::new(metrics_port);
//! let metrics = use_case.execute().await?;
//! println!("Documents indexed: {}", metrics.documents_indexed);
//! println!("Searches performed: {}", metrics.searches_performed);
//! ```

use crate::application::ports::MetricsPort;
use crate::features::metrics::dto::MetricsSnapshotDto;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct GetMetricsUseCase {
    metrics: Arc<dyn MetricsPort>,
}

impl GetMetricsUseCase {
    pub fn new(metrics: Arc<dyn MetricsPort>) -> Self {
        Self { metrics }
    }

    pub async fn execute(&self) -> Result<MetricsSnapshotDto, AppError> {
        let snapshot = self.metrics.snapshot().await;

        Ok(MetricsSnapshotDto {
            documents_indexed: snapshot.documents_indexed,
            searches_performed: snapshot.searches_performed,
            qa_queries: snapshot.qa_queries,
            cache_hits: snapshot.cache_hits,
            cache_misses: snapshot.cache_misses,
            avg_search_time_ms: snapshot.avg_search_time_ms,
            avg_qa_time_ms: snapshot.avg_qa_time_ms,
            uptime_seconds: snapshot.uptime_seconds,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::MetricsSnapshotData;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct MockMetricsPort {
        documents_indexed: AtomicU64,
        searches_performed: AtomicU64,
        qa_queries: AtomicU64,
        cache_hits: AtomicU64,
        cache_misses: AtomicU64,
        avg_search_time_ms: f64,
        avg_qa_time_ms: f64,
        uptime_seconds: AtomicU64,
    }

    impl MockMetricsPort {
        fn new(
            documents_indexed: u64,
            searches_performed: u64,
            qa_queries: u64,
            cache_hits: u64,
            cache_misses: u64,
            avg_search_time_ms: f64,
            avg_qa_time_ms: f64,
            uptime_seconds: u64,
        ) -> Self {
            Self {
                documents_indexed: AtomicU64::new(documents_indexed),
                searches_performed: AtomicU64::new(searches_performed),
                qa_queries: AtomicU64::new(qa_queries),
                cache_hits: AtomicU64::new(cache_hits),
                cache_misses: AtomicU64::new(cache_misses),
                avg_search_time_ms,
                avg_qa_time_ms,
                uptime_seconds: AtomicU64::new(uptime_seconds),
            }
        }

        fn empty() -> Self {
            Self::new(0, 0, 0, 0, 0, 0.0, 0.0, 0)
        }
    }

    #[async_trait]
    impl MetricsPort for MockMetricsPort {
        async fn snapshot(&self) -> MetricsSnapshotData {
            MetricsSnapshotData {
                documents_indexed: self.documents_indexed.load(Ordering::SeqCst),
                searches_performed: self.searches_performed.load(Ordering::SeqCst),
                qa_queries: self.qa_queries.load(Ordering::SeqCst),
                cache_hits: self.cache_hits.load(Ordering::SeqCst),
                cache_misses: self.cache_misses.load(Ordering::SeqCst),
                avg_search_time_ms: self.avg_search_time_ms,
                avg_qa_time_ms: self.avg_qa_time_ms,
                uptime_seconds: self.uptime_seconds.load(Ordering::SeqCst),
            }
        }
    }

    #[tokio::test]
    async fn test_get_metrics_empty() {
        let mock_metrics = Arc::new(MockMetricsPort::empty());
        let use_case = GetMetricsUseCase::new(mock_metrics);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let metrics = result.unwrap();
        assert_eq!(metrics.documents_indexed, 0);
        assert_eq!(metrics.searches_performed, 0);
        assert_eq!(metrics.qa_queries, 0);
        assert_eq!(metrics.cache_hits, 0);
        assert_eq!(metrics.cache_misses, 0);
        assert_eq!(metrics.avg_search_time_ms, 0.0);
        assert_eq!(metrics.avg_qa_time_ms, 0.0);
        assert_eq!(metrics.uptime_seconds, 0);
    }

    #[tokio::test]
    async fn test_get_metrics_with_data() {
        let mock_metrics = Arc::new(MockMetricsPort::new(
            100,   // documents_indexed
            500,   // searches_performed
            50,    // qa_queries
            400,   // cache_hits
            100,   // cache_misses
            25.5,  // avg_search_time_ms
            150.0, // avg_qa_time_ms
            3600,  // uptime_seconds (1 hour)
        ));
        let use_case = GetMetricsUseCase::new(mock_metrics);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let metrics = result.unwrap();
        assert_eq!(metrics.documents_indexed, 100);
        assert_eq!(metrics.searches_performed, 500);
        assert_eq!(metrics.qa_queries, 50);
        assert_eq!(metrics.cache_hits, 400);
        assert_eq!(metrics.cache_misses, 100);
        assert_eq!(metrics.avg_search_time_ms, 25.5);
        assert_eq!(metrics.avg_qa_time_ms, 150.0);
        assert_eq!(metrics.uptime_seconds, 3600);
    }

    #[tokio::test]

    async fn test_get_metrics_high_cache_hit_rate() {
        let mock_metrics = Arc::new(MockMetricsPort::new(
            1000,  // documents_indexed
            10000, // searches_performed
            500,   // qa_queries
            9500,  // cache_hits
            500,   // cache_misses
            10.0,  // avg_search_time_ms
            100.0, // avg_qa_time_ms
            86400, // uptime_seconds (1 day)
        ));
        let use_case = GetMetricsUseCase::new(mock_metrics);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let metrics = result.unwrap();
        let hit_rate =
            metrics.cache_hits as f64 / (metrics.cache_hits + metrics.cache_misses) as f64;
        assert!(
            hit_rate >= 0.95,
            "Expected hit rate >= 0.95, got {}",
            hit_rate
        );
    }

    #[tokio::test]
    async fn test_get_metrics_large_numbers() {
        let mock_metrics = Arc::new(MockMetricsPort::new(
            1_000_000, // documents_indexed
            5_000_000, // searches_performed
            100_000,   // qa_queries
            4_000_000, // cache_hits
            1_000_000, // cache_misses
            15.7,      // avg_search_time_ms
            200.5,     // avg_qa_time_ms
            2_592_000, // uptime_seconds (30 days)
        ));
        let use_case = GetMetricsUseCase::new(mock_metrics);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let metrics = result.unwrap();
        assert_eq!(metrics.documents_indexed, 1_000_000);
        assert_eq!(metrics.searches_performed, 5_000_000);
        assert_eq!(metrics.qa_queries, 100_000);
    }

    #[tokio::test]
    async fn test_get_metrics_fractional_times() {
        let mock_metrics = Arc::new(MockMetricsPort::new(
            50,     // documents_indexed
            200,    // searches_performed
            20,     // qa_queries
            150,    // cache_hits
            50,     // cache_misses
            0.5,    // avg_search_time_ms (very fast)
            999.99, // avg_qa_time_ms
            120,    // uptime_seconds
        ));
        let use_case = GetMetricsUseCase::new(mock_metrics);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let metrics = result.unwrap();
        assert_eq!(metrics.avg_search_time_ms, 0.5);
        assert_eq!(metrics.avg_qa_time_ms, 999.99);
    }

    #[tokio::test]
    async fn test_get_metrics_no_cache_activity() {
        let mock_metrics = Arc::new(MockMetricsPort::new(
            100, // documents_indexed
            0,   // searches_performed
            0,   // qa_queries
            0,   // cache_hits
            0,   // cache_misses
            0.0, // avg_search_time_ms
            0.0, // avg_qa_time_ms
            60,  // uptime_seconds
        ));
        let use_case = GetMetricsUseCase::new(mock_metrics);

        let result = use_case.execute().await;

        assert!(result.is_ok());
        let metrics = result.unwrap();
        assert_eq!(metrics.searches_performed, 0);
        assert_eq!(metrics.cache_hits, 0);
        assert_eq!(metrics.cache_misses, 0);
    }
}
