import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type { ApiResult, CacheMetrics } from '@/types';

export const cacheApi = {
  clearSearchCache: async (): Promise<ApiResult<void>> =>
    apiCall<void>('clear_search_cache'),

  clearCache: async (): Promise<ApiResult<void>> =>
    apiCall<void>('clear_cache'),

  getCacheStats: async (): Promise<ApiResult<Wire.SearchCacheStats>> =>
    apiCall<Wire.SearchCacheStats>('get_cache_stats'),

  getCacheMetrics: async (): Promise<ApiResult<CacheMetrics>> =>
    apiCall<Wire.CacheMetrics>('get_cache_metrics'),
};
