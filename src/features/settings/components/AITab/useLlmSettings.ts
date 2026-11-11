/**
 * Shared LLM settings context for AI sub-tabs.
 *
 * Reads go through `useSettingsQuery`; writes go through
 * `useUpdateSettingsMutation`, which invalidates the cache after a successful
 * update. The frontend does not maintain a second copy of backend settings.
 *
 * The provider still exists so the sub-tabs share one context value; the query
 * cache is what makes it a single fetch.
 */

import { createContext, useContext, useCallback, useEffect, useMemo, useRef } from 'react';

import { useSettingsQuery, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import { useApi } from '@/contexts/ApiContext';
import type { JsonValue, RemoteServer } from '@/lib/bindings';
import { toast } from '@/stores/toastStore';
import type { LLMSettings as ApiLLMSettings } from '@/types/api/settings';

export type LlmSettingsContextValue = {
  llmSettings: ApiLLMSettings | null;
  isLoading: boolean;
  saveLlmUpdates: (updates: Partial<ApiLLMSettings>) => Promise<boolean>;
  /** Store (or with null, delete) a remote server's auth header value. Write-only: settings report whether one is set. */
  saveAuthHeaderValue: (server: RemoteServer, value: string | null) => Promise<boolean>;
  /** Re-read the settings after a failed load. Backs the "Retry" affordance. */
  reload: () => void;
};

export const LlmSettingsContext = createContext<LlmSettingsContextValue | null>(null);

export function useLlmSettingsProvider(): LlmSettingsContextValue {
  const api = useApi();
  const { data, isPending, error, refetch } = useSettingsQuery();
  const { mutateAsync } = useUpdateSettingsMutation();

  const llmSettings = data?.llm ?? null;

  // One toast per failed load, not one per render. `error` is stable between
  // attempts, so the ref only lets a genuinely new failure through.
  const reportedError = useRef<unknown>(null);
  useEffect(() => {
    if (!error || reportedError.current === error) return;
    reportedError.current = error;
    toast.error('Failed to load chat settings', { message: error.message });
  }, [error]);

  const reload = useCallback(() => {
    void refetch();
  }, [refetch]);

  const saveLlmUpdates = useCallback(
    async (updates: Partial<ApiLLMSettings>): Promise<boolean> => {
      try {
        await mutateAsync({
          category: 'llm',
          updates: updates as unknown as Record<string, JsonValue>,
        });
        return true;
      } catch (mutationError) {
        toast.error('Failed to update chat settings', {
          message: mutationError instanceof Error ? mutationError.message : String(mutationError),
        });
        return false;
      }
    },
    [mutateAsync]
  );

  const saveAuthHeaderValue = useCallback(
    async (server: RemoteServer, value: string | null): Promise<boolean> => {
      const result = await api.saveRemoteAuthHeaderValue(server, value);
      if (!result.ok) {
        toast.error('Failed to update the authentication header', { message: result.error });
        return false;
      }
      await refetch();
      return true;
    },
    [api, refetch]
  );

  return useMemo(
    () => ({ llmSettings, isLoading: isPending, saveLlmUpdates, saveAuthHeaderValue, reload }),
    [llmSettings, isPending, saveLlmUpdates, saveAuthHeaderValue, reload]
  );
}

export function useLlmSettings(): LlmSettingsContextValue {
  const ctx = useContext(LlmSettingsContext);
  if (!ctx) {
    throw new Error('useLlmSettings must be used within an LlmSettingsProvider');
  }
  return ctx;
}
