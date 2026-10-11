import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type { RerankerStatusDto } from '@/lib/bindings';

export const RERANKER_STATUS_QUERY_KEY = ['reranker-status'] as const;

/**
 * Whether reranking will actually happen.
 *
 * The setting and the model are independent, and only the pair of them decides
 * the outcome: switched on with nothing installed, search returns its
 * unreranked shortlist and says nothing. `active` is the field to render from.
 */
export function useRerankerStatusQuery() {
  return useQuery<RerankerStatusDto>({
    queryKey: RERANKER_STATUS_QUERY_KEY,
    queryFn: async () => {
      const result = await VaultAPI.getRerankerStatus();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    staleTime: 60_000,
  });
}

/**
 * Fetch the reranker model.
 *
 * The command reports the state it found on disk afterwards rather than
 * assuming the download landed, so its answer seeds the cache directly.
 */
export function useDownloadRerankerMutation() {
  const queryClient = useQueryClient();
  return useMutation<RerankerStatusDto, Error, void>({
    mutationFn: async () => {
      const result = await VaultAPI.downloadReranker();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    onSuccess: (status) => {
      queryClient.setQueryData(RERANKER_STATUS_QUERY_KEY, status);
    },
  });
}
