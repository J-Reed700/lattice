/**
 * Shared LLM settings context for AI sub-tabs.
 *
 * The settings repository is the single source of truth (CLAUDE.md Repository
 * Barrier rule 3): reads go through `useSettingsQuery`, writes through
 * `useUpdateSettingsMutation`, and the mutation invalidates the cache so the
 * next render shows what the backend actually stored. There is no local mirror
 * to drift, no optimistic copy to roll back, and no manual reload token.
 *
 * The provider still exists so the sub-tabs share one context value; the query
 * cache is what makes it a single fetch.
 */

import { createContext, useContext, useCallback, useEffect, useMemo, useRef } from 'react';

import { useSettingsQuery, useUpdateSettingsMutation } from '../../../hooks/queries/useSettingsQuery';
import { toast } from '../../../stores/toastStore';

import type { LLMSettings as ApiLLMSettings } from '../../../types/api/settings';

export type LlmSettingsContextValue = {
  llmSettings: ApiLLMSettings | null;
  isLoading: boolean;
  saveLlmUpdates: (updates: Partial<ApiLLMSettings>) => Promise<boolean>;
  /** Re-read the settings after a failed load. Backs the "Retry" affordance. */
  reload: () => void;
};

export const LlmSettingsContext = createContext<LlmSettingsContextValue | null>(null);

export function useLlmSettingsProvider(): LlmSettingsContextValue {
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
        await mutateAsync({ category: 'llm', updates });
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

  return useMemo(
    () => ({ llmSettings, isLoading: isPending, saveLlmUpdates, reload }),
    [llmSettings, isPending, saveLlmUpdates, reload]
  );
}

export function useLlmSettings(): LlmSettingsContextValue {
  const ctx = useContext(LlmSettingsContext);
  if (!ctx) {
    throw new Error('useLlmSettings must be used within an LlmSettingsProvider');
  }
  return ctx;
}
