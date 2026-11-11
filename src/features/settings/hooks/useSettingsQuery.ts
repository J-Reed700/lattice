import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { LEARNING_CLAIM_REVIEW_ROOT } from '@/features/learning/api/learningQueryKeys';
import { CLOUD_MODELS_QUERY_KEY } from '@/features/model/hooks/useCloudModels';
import VaultAPI, { type ApiClient } from '@/lib/api';
import type { UpdateSettingsRequest } from '@/lib/bindings';
import { SETTINGS_QUERY_KEY } from '@/lib/queryClient';
import type { AppSettings } from '@/types/api/settings';

export { SETTINGS_QUERY_KEY };

export const settingsQueryOptions = (api: ApiClient = VaultAPI) =>
  queryOptions<AppSettings>({
    queryKey: SETTINGS_QUERY_KEY,
    queryFn: async () => {
      const result = await api.getSettings();
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    staleTime: 60_000,
  });

export function useSettingsQuery() {
  const api = useApi();
  return useQuery(settingsQueryOptions(api));
}

export type UpdateSettingsArgs = Pick<UpdateSettingsRequest, 'updates'> & Partial<Pick<UpdateSettingsRequest, 'category'>>;

export function useUpdateSettingsMutation() {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation<AppSettings, Error, UpdateSettingsArgs>({
    mutationFn: async (args) => {
      const payload: UpdateSettingsRequest = { category: args.category ?? null, updates: args.updates };
      const result = await api.updateSettings(payload);
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    onSuccess: (data) => {
      const previous = queryClient.getQueryData<AppSettings>(SETTINGS_QUERY_KEY);
      const checkerChanged = JSON.stringify(previous?.llm.learningModel) !== JSON.stringify(data.llm.learningModel);
      // The command answers with the whole persisted document, so seed the
      // cache with it for an immediate render, then invalidate so the next
      // read is the repository's own answer rather than this response.
      queryClient.setQueryData(SETTINGS_QUERY_KEY, data);
      void queryClient.invalidateQueries({ queryKey: SETTINGS_QUERY_KEY });
      if (checkerChanged) void queryClient.invalidateQueries({ queryKey: LEARNING_CLAIM_REVIEW_ROOT });
    },
  });
}

/**
 * Settings writes that replace the document wholesale. Each answers with the
 * command's result; a success refreshes the settings cache so every tab reads
 * the backend's state.
 */
export function useSettingsTransfer() {
  const api = useApi();
  const queryClient = useQueryClient();
  const refreshOnSuccess = (result: { ok: boolean }) => {
    if (result.ok) {
      void queryClient.invalidateQueries({ queryKey: SETTINGS_QUERY_KEY });
      void queryClient.invalidateQueries({ queryKey: LEARNING_CLAIM_REVIEW_ROOT });
    }
  };
  const reset = useMutation({ mutationFn: () => api.resetSettings(), onSuccess: refreshOnSuccess });
  const importJson = useMutation({
    mutationFn: (json: string) => api.importSettings(json, false),
    onSuccess: refreshOnSuccess,
  });
  const exportJson = useMutation({ mutationFn: () => api.exportSettings() });
  return { resetSettings: reset.mutateAsync, importSettings: importJson.mutateAsync, exportSettings: exportJson.mutateAsync };
}

/**
 * Stores a cloud provider's API key in the system keychain, never in the
 * settings document. Answers with the command's result.
 */
export function useCloudApiKeyMutation() {
  const api = useApi();
  const queryClient = useQueryClient();
  const save = useMutation({
    mutationFn: ({ provider, key }: { provider: string; key: string }) =>
      api.setCloudApiKey(provider, key),
    onSuccess: async (result, { provider }) => {
      if (!result.ok) return;
      const queryKey = [...CLOUD_MODELS_QUERY_KEY, provider];
      // Discard an old key's in-flight response and cached list before the
      // new credential is used, including when the replacement is invalid.
      await queryClient.cancelQueries({ queryKey });
      void queryClient.resetQueries({ queryKey });
    },
  });
  return { setCloudApiKey: save.mutateAsync };
}
