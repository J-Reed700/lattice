import { useQuery, type UseQueryOptions } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import VaultAPI, { type ApiClient } from '@/lib/api';
import { describeFailure } from '@/lib/errorHandling';
import { type SearchResult } from '@/types';

/** The command palette's live search: the ten best hybrid matches. */
export const quickSearchDocuments = (query: string, api: ApiClient = VaultAPI) =>
  api.searchDocuments({ query, limit: 10, searchMode: 'hybrid' });

export interface UseSearchQueryParams {
  query: string;
  mode: 'semantic' | 'keyword' | 'hybrid';
  limit?: number;
}

export const useSearchQuery = (
  params: UseSearchQueryParams,
  options?: Omit<UseQueryOptions<SearchResult[], Error>, 'queryKey' | 'queryFn'>
) => {
  const api = useApi();
  return useQuery<SearchResult[], Error>({
    queryKey: ['search', params.query, params.mode, params.limit],
    queryFn: async () => {
      if (!params.query || params.query.trim().length === 0) {
        return [];
      }

      const result = await api.searchHybrid(
        params.query,
        params.limit || 20,
        params.mode
      );

      if (result.ok) {
        return result.data;
      }

      throw new Error(describeFailure(result) || 'Search failed');
    },
    enabled: params.query.trim().length > 0,
    staleTime: 5 * 60 * 1000,
    ...options,
  });
};
