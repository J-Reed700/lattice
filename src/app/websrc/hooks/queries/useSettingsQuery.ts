import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';

import type { AppSettings } from '@/types/api/settings';

export const SETTINGS_QUERY_KEY = ['settings'] as const;

export function useSettingsQuery() {
  return useQuery<AppSettings>({
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
}

export interface UpdateSettingsArgs {
  category?: string;
  updates: Record<string, unknown>;
}

export function useUpdateSettingsMutation() {
  const queryClient = useQueryClient();
  return useMutation<AppSettings, Error, UpdateSettingsArgs>({
    mutationFn: async (args) => {
      const payload: Record<string, unknown> = args.category
        ? { category: args.category, updates: args.updates }
        : { updates: args.updates };
      const result = await VaultAPI.updateSettings(payload);
      if (!result.ok) {
        throw new Error(result.error);
      }
      return result.data;
    },
    onSuccess: (data) => {
      queryClient.setQueryData(SETTINGS_QUERY_KEY, data);
    },
  });
}
