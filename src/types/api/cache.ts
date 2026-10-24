export type CacheStats = import('../../lib/bindings').SearchCacheStats;

export type CacheMetrics = import('../../lib/bindings').CacheMetrics;

export interface LLMCacheStats {
  hits: number;
  misses: number;
  totalTokensSaved: number;
  hitRatePercent: number;
}
