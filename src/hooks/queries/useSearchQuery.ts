import { useQuery, type UseQueryOptions } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import { type SearchResult } from '@/types';

export interface UseSearchQueryParams {
  query: string;
  mode: 'semantic' | 'keyword' | 'hybrid';
  limit?: number;
}

export const useSearchQuery = (
  params: UseSearchQueryParams,
  options?: Omit<UseQueryOptions<SearchResult[], Error>, 'queryKey' | 'queryFn'>
) => useQuery<SearchResult[], Error>({
    queryKey: ['search', params.query, params.mode, params.limit],
    queryFn: async () => {
      if (!params.query || params.query.trim().length === 0) {
        return [];
      }

      const result = await VaultAPI.searchHybrid(
        params.query,
        params.limit || 20,
        params.mode
      );

      if (result.ok) {
        return result.data;
      }

      throw new Error(result.error || 'Search failed');
    },
    enabled: params.query.trim().length > 0,
    staleTime: 5 * 60 * 1000,
    ...options,
  });
