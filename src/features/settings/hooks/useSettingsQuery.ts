import { useContext } from 'react';

import {
  QueryClient,
  QueryClientContext,
  queryOptions,
  useMutation,
  useQuery,
} from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type { UpdateSettingsRequest } from '@/lib/bindings';
import type { AppSettings } from '@/types/api/settings';

export const SETTINGS_QUERY_KEY = ['settings'] as const;

export const settingsQueryOptions = () =>
  queryOptions<AppSettings>({
    queryKey: SETTINGS_QUERY_KEY,
    queryFn: async () => {
      const result = await VaultAPI.getSettings();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    staleTime: 60_000,
  });

const fallbackQueryClient = new QueryClient({
  defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
});

export function useSettingsQuery() {
  const contextClient = useContext(QueryClientContext);
  return useQuery(
    { ...settingsQueryOptions(), enabled: Boolean(contextClient) },
    contextClient ?? fallbackQueryClient
  );
}

export type UpdateSettingsArgs = Pick<UpdateSettingsRequest, 'updates'> & Partial<Pick<UpdateSettingsRequest, 'category'>>;

export function useUpdateSettingsMutation() {
  const contextClient = useContext(QueryClientContext);
  const queryClient = contextClient ?? fallbackQueryClient;
  return useMutation<AppSettings, Error, UpdateSettingsArgs>(
    {
      mutationFn: async (args) => {
        const payload: UpdateSettingsRequest = { category: args.category ?? null, updates: args.updates };
        const result = await VaultAPI.updateSettings(payload);
        if (!result.ok) {
          throw new Error(result.error);
        }
        return result.data;
      },
      onSuccess: (data) => {
        // The command answers with the whole persisted document, so seed the
        // cache with it for an immediate render, then invalidate so the next
        // read is the repository's own answer rather than this response.
        queryClient.setQueryData(SETTINGS_QUERY_KEY, data);
        void queryClient.invalidateQueries({ queryKey: SETTINGS_QUERY_KEY });
      },
    },
    queryClient
  );
}

/**
 * Settings writes that replace the document wholesale. Each answers with the
 * command's result; a success refreshes the settings cache so every tab reads
 * the backend's state.
 */
export function useSettingsTransfer() {
  const queryClient = useContext(QueryClientContext) ?? fallbackQueryClient;
  const refreshOnSuccess = (result: { ok: boolean }) => {
    if (result.ok) void queryClient.invalidateQueries({ queryKey: SETTINGS_QUERY_KEY });
  };
  const reset = useMutation({ mutationFn: () => VaultAPI.resetSettings(), onSuccess: refreshOnSuccess }, queryClient);
  const importJson = useMutation(
    {
      mutationFn: (json: string) => VaultAPI.importSettings(json, false),
      onSuccess: refreshOnSuccess,
    },
    queryClient
  );
  const exportJson = useMutation({ mutationFn: () => VaultAPI.exportSettings() }, queryClient);
  return { resetSettings: reset.mutateAsync, importSettings: importJson.mutateAsync, exportSettings: exportJson.mutateAsync };
}

/**
 * Adds or removes a watched folder. The backend validates the path and
 * serializes concurrent changes; a success refreshes the settings cache.
 */
export function useWatchFolderMutations() {
  const queryClient = useContext(QueryClientContext) ?? fallbackQueryClient;
  const refreshOnSuccess = (result: { ok: boolean }) => {
    if (result.ok) void queryClient.invalidateQueries({ queryKey: SETTINGS_QUERY_KEY });
  };
  const add = useMutation(
    { mutationFn: (path: string) => VaultAPI.addWatchFolder(path), onSuccess: refreshOnSuccess },
    queryClient
  );
  const remove = useMutation(
    { mutationFn: (path: string) => VaultAPI.removeWatchFolder(path), onSuccess: refreshOnSuccess },
    queryClient
  );
  return { addWatchFolder: add.mutateAsync, removeWatchFolder: remove.mutateAsync };
}

/**
 * Stores a cloud provider's API key in the system keychain, never in the
 * settings document. Answers with the command's result.
 */
export function useCloudApiKeyMutation() {
  const queryClient = useContext(QueryClientContext) ?? fallbackQueryClient;
  const save = useMutation(
    {
      mutationFn: ({ provider, key }: { provider: string; key: string }) =>
        VaultAPI.setCloudApiKey(provider, key),
    },
    queryClient
  );
  return { setCloudApiKey: save.mutateAsync };
}
