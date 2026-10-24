import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult, CacheMetrics } from '@/types';

export const cacheApi = {
  /**
   * Clears the search results cache.
   * Forces fresh results on next search instead of using cached data.
   *
   * @returns Void on success
   */
  clearSearchCache: async (): Promise<ApiResult<void>> =>
    apiCall<void>('clear_search_cache'),

  /**
   * Clears all caches (search, embeddings, etc.).
   * Comprehensive cache clear for troubleshooting or memory management.
   *
   * @returns Void on success
   */
  clearCache: async (): Promise<ApiResult<void>> =>
    apiCall<void>('clear_cache'),

  /**
   * Retrieves cache statistics.
   * Includes hit rate, size, and entry count for all caches.
   *
   * @returns Cache statistics object
   */
  getCacheStats: async (): Promise<ApiResult<Wire.SearchCacheStats>> =>
    apiCall<Wire.SearchCacheStats>('get_cache_stats'),

  /**
   * Gets detailed cache performance metrics.
   * Returns metrics for monitoring and optimization.
   *
   * @returns Cache metrics object with detailed statistics
   */
  getCacheMetrics: async (): Promise<ApiResult<CacheMetrics>> =>
    apiCall<Wire.CacheMetrics>('get_cache_metrics'),
};
