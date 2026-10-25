import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
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
  const api = useApi();
  return useQuery<RerankerStatusDto>({
    queryKey: RERANKER_STATUS_QUERY_KEY,
    queryFn: async () => {
      const result = await api.getRerankerStatus();
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
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation<RerankerStatusDto, Error, void>({
    mutationFn: async () => {
      const result = await api.downloadReranker();
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
