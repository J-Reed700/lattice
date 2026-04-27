/**
 * Cache API Types
 *
 * Type definitions for cache statistics and metrics.
 * These types match the Rust backend structures from commands/cache.rs
 */

export interface CacheStats {
  size: number;
  capacity: number;
  hits: number;
  misses: number;
  total_time_saved_ms: number;
  hit_rate: number;
}

export interface CacheMetrics {
  hits: number;
  misses: number;
  total_time_saved_ms: number;
  hit_rate: number;
}

export interface LLMCacheStats {
  hits: number;
  misses: number;
  totalTokensSaved: number;
  hitRatePercent: number;
}
