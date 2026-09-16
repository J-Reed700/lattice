import { useQuery, type UseQueryOptions } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';

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

function startOfDayMs(date: Date): number {
  const copy = new Date(date);
  copy.setHours(0, 0, 0, 0);
  return copy.getTime();
}

export function useChatEmptyStateStats(
  options?: Omit<UseQueryOptions<ChatEmptyStateStats, Error>, 'queryKey' | 'queryFn'>
) {
  return useQuery<ChatEmptyStateStats, Error>({
    queryKey: ['chat-empty-state-stats'],
    queryFn: async () => {
      const [statsResult, recentResult] = await Promise.all([
        VaultAPI.getIndexingStats(),
        VaultAPI.listAllDocuments(10000),
      ]);

      const totalDocuments = statsResult.ok
        ? statsResult.data.indexedDocuments ?? 0
        : 0;

      let documentsAddedToday = 0;
      let documentsAddedThisWeek = 0;

      if (recentResult.ok) {
        const now = new Date();
        const todayStart = startOfDayMs(now);
        const weekStart = todayStart - 6 * 24 * 60 * 60 * 1000;

        for (const doc of recentResult.data) {
          const ts = Date.parse(doc.indexedAt);
          if (!Number.isFinite(ts)) continue;
          if (ts >= todayStart) {
            documentsAddedToday += 1;
          }
          if (ts >= weekStart) {
            documentsAddedThisWeek += 1;
          }
        }
      }

      return {
        totalDocuments,
        documentsAddedToday,
        documentsAddedThisWeek,
      };
    },
    staleTime: 60 * 1000,
    refetchInterval: 5 * 60 * 1000,
    ...options,
  });
}
