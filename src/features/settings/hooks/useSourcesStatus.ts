import { useContext } from 'react';

import { QueryClient, QueryClientContext, useMutation, useQuery } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type { KeyServiceDto, SourcesStatusDto } from '@/lib/bindings';

export const SOURCES_STATUS_QUERY_KEY = ['sources-status'] as const;

const fallbackQueryClient = new QueryClient({
  defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
});

function useClient() {
  return useContext(QueryClientContext) ?? fallbackQueryClient;
}

/**
 * Settings → Sources: every source finder with its need, privacy line,
 * switch, gate and health, and which key services have a key stored. Health
 * moves as searches run, so it is read fresh whenever the tab opens.
 */
export function useSourcesStatus() {
  const queryClient = useClient();
  return useQuery<SourcesStatusDto>(
    {
      queryKey: SOURCES_STATUS_QUERY_KEY,
      queryFn: async () => {
        const result = await VaultAPI.getSourcesStatus();
        if (!result.ok) throw new Error(result.error);
        return result.data;
      },
      staleTime: 0,
    },
    queryClient,
  );
}

/** Re-reads the status after a settings write re-registered the finders. */
export function useRefreshSourcesStatus() {
  const queryClient = useClient();
  return () => queryClient.invalidateQueries({ queryKey: SOURCES_STATUS_QUERY_KEY });
}

/**
 * Saves or removes a source-finding key in the system keychain. Each answers
 * with the new status, which says only whether a key is stored.
 */
export function useSourceKeyMutations() {
  const queryClient = useClient();
  const settle = (status: SourcesStatusDto) =>
    queryClient.setQueryData(SOURCES_STATUS_QUERY_KEY, status);
  const save = useMutation<SourcesStatusDto, Error, { service: KeyServiceDto; key: string }>(
    {
      mutationFn: async ({ service, key }) => {
        const result = await VaultAPI.saveSourceKey(service, key);
        if (!result.ok) throw new Error(result.error);
        return result.data;
      },
      onSuccess: settle,
    },
    queryClient,
  );
  const remove = useMutation<SourcesStatusDto, Error, KeyServiceDto>(
    {
      mutationFn: async (service) => {
        const result = await VaultAPI.removeSourceKey(service);
        if (!result.ok) throw new Error(result.error);
        return result.data;
      },
      onSuccess: settle,
    },
    queryClient,
  );
  return { saveKey: save, removeKey: remove };
}
