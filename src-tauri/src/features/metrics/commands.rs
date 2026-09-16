//! Application Metrics and Telemetry Commands
//!
//! Thin command controllers for collecting and reporting application metrics following
//! DDD pattern. Provides comprehensive telemetry data for monitoring application health,
//! performance, and usage patterns with rate limiting to prevent resource exhaustion.
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
use crate::shared::error::{AppError, Result};
use tauri::State;

/// Retrieves comprehensive snapshot of all application metrics
///
/// Returns a complete snapshot of application metrics including performance counters,
/// usage statistics, health indicators, and resource utilization. Rate limited to
/// prevent resource exhaustion from excessive metrics collection.
///
/// # Arguments
///
/// * `container` - Service container with use cases and security context
///
/// # Returns
///
/// * `Ok(MetricsSnapshotDto)` - Complete metrics snapshot with all telemetry data
/// * `Err(AppError::RateLimitExceeded)` - Too many metrics requests
/// * `Err(AppError)` - Metrics collection failed
///
/// # Errors
///
/// * `AppError::RateLimitExceeded` - Rate limit exceeded (prevents DoS)
/// * `AppError::Other` - Metrics collection or aggregation failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface MetricsSnapshot {
///   // Performance Metrics
///   searchQueryTimeMs: number;        // Avg search query time
///   embeddingGenerationTimeMs: number; // Avg embedding generation time
///   cacheHitRate: number;             // Cache effectiveness (0.0-1.0)
///
///   // Usage Metrics
///   totalSearches: number;            // Total searches since startup
///   totalIndexingOps: number;         // Total indexing operations
///   totalDocuments: number;           // Total indexed documents
///   totalChunks: number;              // Total document chunks
///
///   // Health Metrics
///   errorRate: number;                // Error rate (0.0-1.0)
///   uptime: number;                   // Application uptime (seconds)
///
///   // Resource Metrics
///   memoryUsageBytes: number;         // Current memory usage
///   cpuUsagePercent: number;          // Current CPU usage
///   databaseSizeBytes: number;        // Database file size
///
///   timestamp: string;                // ISO 8601 timestamp
/// }
///
/// // Get metrics snapshot
/// const metrics = await invoke<MetricsSnapshot>('get_metrics');
///
/// console.log('Performance:', {
///   searchTime: `${metrics.searchQueryTimeMs}ms`,
///   cacheHitRate: `${(metrics.cacheHitRate * 100).toFixed(1)}%`
/// });
///
/// console.log('Usage:', {
///   searches: metrics.totalSearches,
///   documents: metrics.totalDocuments,
///   uptime: `${(metrics.uptime / 3600).toFixed(1)}h`
/// });
///
/// console.log('Resources:', {
///   memory: `${(metrics.memoryUsageBytes / 1024 / 1024).toFixed(0)}MB`,
///   cpu: `${metrics.cpuUsagePercent.toFixed(1)}%`,
///   dbSize: `${(metrics.databaseSizeBytes / 1024 / 1024).toFixed(0)}MB`
/// });
///
/// // Periodic metrics collection for monitoring
/// setInterval(async () => {
///   const metrics = await invoke<MetricsSnapshot>('get_metrics');
///   sendToMonitoringService(metrics);
/// }, 60000); // Every minute
///
/// // Display metrics dashboard
/// const updateDashboard = async () => {
///   const metrics = await invoke<MetricsSnapshot>('get_metrics');
///   updatePerformanceChart(metrics);
///   updateUsageStats(metrics);
///   updateResourceGauges(metrics);
/// };
/// ```
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
/// # Security
///
/// **Rate Limiting (CWE-770)**: Limits metrics collection to prevent DoS
/// - Max rate configurable (default: 60 requests/minute)
/// - Shared rate limit with health checks
///
/// # Use Cases
///
/// - **Monitoring Dashboard**: Display real-time application metrics
/// - **Performance Analysis**: Identify bottlenecks and optimization opportunities
/// - **Capacity Planning**: Track resource usage trends
/// - **Alerting**: Trigger alerts based on metric thresholds
/// - **Debugging**: Diagnose issues with telemetry data
///
/// # Performance
///
/// - **Collection Time**: ~10-50ms (depends on metric count)
/// - **Aggregation**: Combines data from multiple sources
/// - **Non-Intrusive**: Metric collection has minimal overhead
/// - **Rate Limited**: Prevents excessive load
///
/// # Architecture
///
/// Thin controller delegating to `GetMetricsUseCase` (DDD pattern)
pub async fn get_metrics(container: State<'_, Container>) -> Result<MetricsSnapshotDto> {
    // 1. Rate limiting (CWE-770 mitigation)
    container
        .security_context()
        .rate_limiters()
        .health_check
        .check_rate_limit("global")
        .await
        .map_err(|e| AppError::RateLimitExceeded(e.to_string()))?;

    // 2. Get use case from container
    let use_case = container.get_metrics_use_case();

    // 3. Execute use case
    let metrics = use_case.execute().await?;

    Ok(metrics)
}
