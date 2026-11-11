import { useContext } from 'react';

import { QueryClient, QueryClientContext, queryOptions, useQuery } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import VaultAPI, { type ApiClient } from '@/lib/api';
import type { RemoteAuthStatusDto } from '@/lib/bindings';

export const REMOTE_AUTH_STATUS_QUERY_KEY = ['remote-auth-status'] as const;

export const NO_REMOTE_AUTH: RemoteAuthStatusDto = { ollama: false, llamaCpp: false };

export const remoteAuthStatusQueryOptions = (api: ApiClient = VaultAPI) =>
  queryOptions<RemoteAuthStatusDto>({
    queryKey: REMOTE_AUTH_STATUS_QUERY_KEY,
    queryFn: async () => {
      const result = await api.getRemoteAuthHeaderStatus();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
  });

const fallbackQueryClient = new QueryClient({
  defaultOptions: { queries: { retry: false } },
});

/** Whether the keyring holds an auth header value for each remote server. Invalidate after saving or clearing one. */
export function useRemoteAuthStatus() {
  const api = useApi();
  const contextClient = useContext(QueryClientContext);
  return useQuery(
    { ...remoteAuthStatusQueryOptions(api), enabled: Boolean(contextClient) },
    contextClient ?? fallbackQueryClient
  );
}
