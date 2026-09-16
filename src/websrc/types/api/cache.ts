/**
 * Cache API Types
 *
 * Type definitions for cache statistics and metrics.
 * These types match the Rust backend structures from commands/cache.rs
 */

export type CacheStats = import('../../lib/bindings').SearchCacheStats;

export type CacheMetrics = import('../../lib/bindings').CacheMetrics;

export interface LLMCacheStats {
  hits: number;
  misses: number;
  totalTokensSaved: number;
  hitRatePercent: number;
}
