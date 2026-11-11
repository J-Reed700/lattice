import { useMemo } from 'react';

import { queryOptions, useQueries, useQuery } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import VaultAPI, { type ApiClient } from '@/lib/api';
import type { CloudProvider } from '@/lib/bindings';
import type { CloudModelCatalogs } from '@/utils/chatModelSelection';

export const CLOUD_MODELS_QUERY_KEY = ['cloud-models'] as const;
const PROVIDERS = ['openai', 'anthropic', 'google'] as const;

export const cloudModelsQueryOptions = (provider: CloudProvider, api: ApiClient = VaultAPI) => queryOptions({
  queryKey: [...CLOUD_MODELS_QUERY_KEY, provider],
  queryFn: async () => {
    const result = await api.listCloudModels(provider);
    if (!result.ok) throw new Error(result.error);
    return result.data;
  },
  staleTime: 5 * 60_000,
  retry: false,
});

export function useCloudModels(provider: CloudProvider) {
  const api = useApi();
  return useQuery(cloudModelsQueryOptions(provider, api));
}

/** Shared with settings; one failed provider never hides another's models. */
export function useCloudModelCatalogs(enabled = true) {
  const api = useApi();
  const queries = useQueries({
    queries: PROVIDERS.map((provider) => ({ ...cloudModelsQueryOptions(provider, api), enabled })),
  });
  const [openai, anthropic, google] = queries.map((query) => query.data);
  const catalogs: CloudModelCatalogs = useMemo(() => ({ openai, anthropic, google }), [openai, anthropic, google]);
  return {
    catalogs,
    isLoading: queries.some((query) => query.isLoading),
    isFetching: queries.some((query) => query.isFetching),
    errors: queries.flatMap((query, index) => query.error ? [{ provider: PROVIDERS[index], message: query.error.message }] : []),
    refresh: () => Promise.all(queries.map((query) => query.refetch())),
  };
}
