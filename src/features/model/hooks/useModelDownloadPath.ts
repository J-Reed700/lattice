import { useCallback, useContext } from 'react';

import { QueryClient, QueryClientContext, useQuery } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';



export const MODEL_DOWNLOAD_PATH_QUERY_KEY = ['model', 'download-path'] as const;

// Settings panes can render without a provider; they still need the path.
const fallbackQueryClient = new QueryClient({
  defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
});

/** Where downloaded models are stored; empty when the backend cannot say. */
export function useModelDownloadPath() {
  const api = useApi();
  const contextClient = useContext(QueryClientContext);
  const query = useQuery(
    {
      queryKey: MODEL_DOWNLOAD_PATH_QUERY_KEY,
      queryFn: async () => {
        try {
          const result = await api.getModelDownloadPath();
          return result.ok ? result.data : '';
        } catch {
          // The command can be missing on older builds. Fall through to the
          // error copy instead of leaving the row stuck on "Loading…".
          return '';
        }
      },
      staleTime: 0,
    },
    contextClient ?? fallbackQueryClient,
  );

  const { refetch } = query;
  const loadModelDownloadPath = useCallback(async () => {
    await refetch();
  }, [refetch]);

  return {
    modelDownloadPath: query.data ?? '',
    isLoadingModelDownloadPath: query.isFetching,
    loadModelDownloadPath,
  };
}
