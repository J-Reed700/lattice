//! Application Metrics and Telemetry Commands
//!
//! Thin command controllers for collecting and reporting application metrics following
//! DDD pattern. Provides comprehensive telemetry data for monitoring application health,
//! performance, and usage patterns.
//!
//! # Commands (1 total)
//!
//! - `get_metrics` - Retrieve snapshot of all application metrics
//!
//! # Metrics Categories
//!
//! - **Performance**: Query execution times, cache hit rates, indexing throughput
//! - **Usage**: Operation counts, active sessions, resource utilization
//! - **Health**: Error rates, service availability, memory/CPU usage
//! - **Storage**: Database size, document count, chunk count
//!
//! # Security Features
//!
//! - **Rate Limiting (CWE-770)**: Prevents DoS via excessive metrics queries
//! - **Read-Only**: Metrics collection is non-intrusive and read-only
//!
//! # Architecture
//!
//! Commands delegate to `GetMetricsUseCase` which aggregates metrics from:
//! - Metrics service (counters, gauges, histograms)
//! - Cache stats (hit rate, time saved)
//! - Database queries (document/chunk counts)
//! - System monitoring (memory, CPU)

use crate::features::metrics::dto::MetricsSnapshotDto;
use crate::interfaces::di::Container;
use lattice_core::shared::error::Result;
use tauri::State;

/// Retrieves comprehensive snapshot of all application metrics
///
/// Returns a complete snapshot of application metrics including performance counters,
/// usage statistics, health indicators, and resource utilization.
///
/// # Errors
///
/// * `AppError::Other` - Metrics collection or aggregation failed
///
/// # Metrics Categories
///
/// **Performance Metrics**:
/// - Average query execution times
/// - Cache hit rates and time saved
/// - Indexing throughput (docs/sec)
/// - Embedding generation rates
///
/// **Usage Metrics**:
/// - Operation counters (searches, indexing, Q&A)
/// - Total indexed content (docs, chunks, bytes)
/// - Active sessions/connections
/// - Feature usage statistics
///
/// **Health Metrics**:
/// - Error rates by category
/// - Service availability indicators
/// - Application uptime
/// - Component status (database, models, cache)
///
/// **Resource Metrics**:
/// - Memory usage (heap, RSS)
/// - CPU utilization
/// - Database size
/// - Cache size
///
/// # Architecture
///
/// Thin controller delegating to `GetMetricsUseCase` (DDD pattern)
pub async fn get_metrics(container: State<'_, Container>) -> Result<MetricsSnapshotDto> {
    let use_case = container.get_metrics_use_case();
    let metrics = use_case.execute().await?;

    Ok(metrics)
}
