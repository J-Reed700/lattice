import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { DOWNLOADS_QUERY_KEY } from '@/features/model/hooks/useDownloads';
import type { RerankerStatusDto } from '@/lib/bindings';
import { toast } from '@/stores/toastStore';

export const RERANKER_STATUS_QUERY_KEY = ['reranker-status'] as const;
export const DOWNLOAD_RERANKER_MUTATION_KEY = ['reranker-download'] as const;
export const REMOVE_RERANKER_MUTATION_KEY = ['reranker-remove'] as const;

/**
 * Whether the reranker's model is on disk, and so whether search reranks, and
 * the path its download reports. Always re-read on mount: a download may have
 * finished while the page was closed.
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
    staleTime: 0,
  });
}

/**
 * Start the reranker download on the shared download manager.
 *
 * The command returns once the download is registered, not when the file
 * lands; the row follows it through the download list. `onSuccess` is on the
 * mutation, so it still runs if the page is left while the call is in flight.
 */
export function useDownloadRerankerMutation() {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation<RerankerStatusDto, Error, void>({
    mutationKey: DOWNLOAD_RERANKER_MUTATION_KEY,
    mutationFn: async () => {
      const result = await api.downloadReranker();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    onSuccess: async (status) => {
      queryClient.setQueryData(RERANKER_STATUS_QUERY_KEY, status);
      if (status.installed) {
        toast.info('Already downloaded', { message: `${status.modelName} is on disk.` });
      } else {
        toast.success('Download started', { message: status.modelName });
      }
      await queryClient.invalidateQueries({ queryKey: DOWNLOADS_QUERY_KEY });
    },
    onError: (error) => {
      toast.error("Couldn't start the download", { message: error.message });
    },
  });
}

/**
 * Delete the reranker's model. The row's control toasts the outcome, so the
 * mutation stays quiet.
 */
export function useRemoveRerankerMutation() {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation<RerankerStatusDto, Error, void>({
    mutationKey: REMOVE_RERANKER_MUTATION_KEY,
    mutationFn: async () => {
      const result = await api.removeReranker();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    onSuccess: async (status) => {
      // Removal deletes the download sessions without telling the UI. Refresh
      // the list first: a status that flips to "not installed" while a
      // just-completed session is still listed would read as "Finishing…".
      await queryClient.invalidateQueries({ queryKey: DOWNLOADS_QUERY_KEY });
      queryClient.setQueryData(RERANKER_STATUS_QUERY_KEY, status);
    },
  });
}
