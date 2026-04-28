/**
 * AppConfig (indexedPaths, excludePatterns, autoIndex, ollama settings)
 * via React Query.
 *
 * Note: AppConfig and Settings are two parallel backend stores that
 * partially overlap (both touch indexing-related fields). Unifying them
 * is a separate cleanup; for now we mirror what the backend exposes.
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';

import type { AppConfig } from '@/types';

export const CONFIG_QUERY_KEY = ['config'] as const;

export function useConfigQuery() {
  return useQuery<AppConfig>({
    queryKey: CONFIG_QUERY_KEY,
    queryFn: async () => {
      const result = await VaultAPI.getConfig();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    staleTime: 60_000,
  });
}

export function useSaveConfigMutation() {
  const queryClient = useQueryClient();
  return useMutation<void, Error, AppConfig>({
    mutationFn: async (config) => {
      const result = await VaultAPI.saveConfig(config);
      if (!result.ok) {
        throw new Error(result.error);
      }
    },
    onSuccess: (_data, config) => {
      // saveConfig returns void; optimistically write the request payload
      // into the cache so subsequent reads don't refetch immediately.
      queryClient.setQueryData(CONFIG_QUERY_KEY, config);
    },
  });
}

export function useAddWatchFolderMutation() {
  const queryClient = useQueryClient();
  return useMutation<void, Error, string>({
    mutationFn: async (path) => {
      const result = await VaultAPI.addWatchFolder(path);
      if (!result.ok) {
        throw new Error(result.error);
      }
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: CONFIG_QUERY_KEY });
    },
  });
}

export function useRemoveWatchFolderMutation() {
  const queryClient = useQueryClient();
  return useMutation<void, Error, string>({
    mutationFn: async (path) => {
      const result = await VaultAPI.removeWatchFolder(path);
      if (!result.ok) {
        throw new Error(result.error);
      }
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: CONFIG_QUERY_KEY });
    },
  });
}
