/**
 * Metrics API Types
 *
 * Type definitions for application metrics and observability.
 * These types match the Rust backend structures from observability/metrics.rs
 */

export interface MetricsSnapshot {
  documentsIndexed: number;    // Matches Rust documents_indexed with camelCase
  searchesPerformed: number;   // Matches Rust searches_performed with camelCase
  qaQueries: number;           // Matches Rust qa_queries with camelCase
  cacheHits: number;
  cacheMisses: number;
  avgSearchTimeMs: number;     // Matches Rust avg_search_time_ms with camelCase
  avgQaTimeMs: number;         // Matches Rust avg_qa_time_ms with camelCase
  uptimeSeconds: number;       // Matches Rust uptime_seconds with camelCase
}

/**
 * System Statistics (DDD)
 *
 * Type definition for DDD system statistics.
 * Matches SystemStatsDto from application/dtos/modules/health_dto.rs
 */
export interface SystemStats {
  total_documents: number;
  total_chunks: number;
  total_tags: number;
  storage_size_bytes: number;
}
