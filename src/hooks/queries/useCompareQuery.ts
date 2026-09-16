import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type { CompareTableDto } from '@/types/api/compare';

/** One cache entry per document-and-column selection. */
export function compareQueryKey(documentIds: string[], columns: string[]) {
  return ['compare', documentIds, columns] as const;
}

/**
 * Builds a comparison table. A run is expensive and non-deterministic, so it
 * never refetches on its own: `staleTime: Infinity`, `retry: false`, and the
 * page refreshes explicitly with `refetch()`.
 */
export function useCompareQuery(
  documentIds: string[],
  columns: string[],
  enabled: boolean,
): UseQueryResult<CompareTableDto, Error> {
  return useQuery<CompareTableDto, Error>({
    queryKey: compareQueryKey(documentIds, columns),
    queryFn: async () => {
      const result = await VaultAPI.compareDocuments({ documentIds, columns });
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    enabled: enabled && documentIds.length >= 2 && columns.length > 0,
    staleTime: Infinity,
    gcTime: 10 * 60_000,
    retry: false,
    refetchOnWindowFocus: false,
  });
}
