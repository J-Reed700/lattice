import { useQuery, type UseQueryOptions } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';



/**
 * useChatEmptyStateStats
 *
 * Small corpus readout for the Chat empty state — "what does the
 * lattice know?". Reuses existing stats endpoints so a failure leaves
 * the empty state degrading gracefully to its base copy.
 */
export interface ChatEmptyStateStats {
  totalDocuments: number;
  documentsAddedToday: number;
  documentsAddedThisWeek: number;
}

export function useChatEmptyStateStats(
  options?: Omit<UseQueryOptions<ChatEmptyStateStats, Error>, 'queryKey' | 'queryFn'>
) {
  const api = useApi();
  return useQuery<ChatEmptyStateStats, Error>({
    queryKey: ['chat-empty-state-stats'],
    queryFn: async () => {
      const [statsResult, shapeResult] = await Promise.all([
        api.getIndexingStats(),
        api.getCorpusShape(),
      ]);

      const totalDocuments = statsResult.ok
        ? statsResult.data.indexedDocuments ?? 0
        : 0;

      return {
        totalDocuments,
        documentsAddedToday: shapeResult.ok ? shapeResult.data.grownToday : 0,
        documentsAddedThisWeek: shapeResult.ok ? shapeResult.data.grownLast7Days : 0,
      };
    },
    staleTime: 60 * 1000,
    refetchInterval: 5 * 60 * 1000,
    ...options,
  });
}
