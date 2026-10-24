/**
 * Statistics and Metrics API Types
 *
 * Type definitions for application metrics and telemetry.
 * These types match the Rust backend structures from application/dtos/modules/metric_dto.rs
 */

/**
 * Single metric with value, unit, and timestamp.
 */
export interface MetricDto {
  /** Metric name (e.g., "search_query_time", "cache_hit_rate") */
  name: string;
  /** Metric value */
  value: number;
  /** Optional unit (e.g., "ms", "bytes", "%") */
  unit: string | null;
  /** Timestamp (ISO 8601) */
  timestamp: string;
}

/**
 * Comprehensive snapshot of all application metrics.
 *
 * Includes performance, usage, health, and resource metrics.
 */
export interface MetricsSnapshotDto {
  // Usage Metrics
  /** Total documents indexed */
  documentsIndexed: number;
  /** Total searches performed */
  searchesPerformed: number;
  /** Total Q&A queries */
  qaQueries: number;

  // Cache Metrics
  /** Number of cache hits */
  cacheHits: number;
  /** Number of cache misses */
  cacheMisses: number;

  // Performance Metrics
  /** Average search time in milliseconds */
  avgSearchTimeMs: number;
  /** Average Q&A time in milliseconds */
  avgQaTimeMs: number;

  // Health Metrics
  /** Application uptime in seconds */
  uptimeSeconds: number;
}

/**
 * Result from recording a metric.
 */
export interface RecordMetricResultDto {
  /** Whether the metric was recorded successfully */
  success: boolean;
  /** Optional message (error message if failed) */
  message: string | null;
}

/**
 * Cache hit rate calculation helper.
 */
export interface CacheStats {
  /** Total cache accesses */
  totalAccesses: number;
  /** Number of hits */
  hits: number;
  /** Number of misses */
  misses: number;
  /** Hit rate as percentage (0-100) */
  hitRatePercent: number;
}

/**
 * Helper to calculate cache stats from metrics snapshot.
 */
export function calculateCacheStats(metrics: MetricsSnapshotDto): CacheStats {
  const totalAccesses = metrics.cacheHits + metrics.cacheMisses;
  const hitRatePercent = totalAccesses > 0
    ? (metrics.cacheHits / totalAccesses) * 100
    : 0;

  return {
    totalAccesses,
    hits: metrics.cacheHits,
    misses: metrics.cacheMisses,
    hitRatePercent,
  };
}

/**
 * Uptime formatted as human-readable string.
 */
export function formatUptime(seconds: number): string {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const secs = seconds % 60;

  if (hours > 0) {
    return `${hours}h ${minutes}m ${secs}s`;
  } else if (minutes > 0) {
    return `${minutes}m ${secs}s`;
  } else {
    return `${secs}s`;
  }
}
