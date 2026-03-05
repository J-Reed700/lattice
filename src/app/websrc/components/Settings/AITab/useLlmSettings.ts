/**
 * Shared LLM settings context for AI sub-tabs.
 *
 * A single provider loads settings once and shares state across all tabs,
 * preventing duplicate API calls and stale cross-tab data.
 */

import { createContext, useContext, useState, useEffect, useCallback } from 'react';

import { VaultAPI } from '../../../lib/api';
import { toast } from '../../../stores/toastStore';

import type { LLMSettings as ApiLLMSettings } from '../../../types/api/settings';

export type LlmSettingsContextValue = {
  llmSettings: ApiLLMSettings | null;
  isLoading: boolean;
  saveLlmUpdates: (updates: Partial<ApiLLMSettings>) => Promise<boolean>;
};

export const LlmSettingsContext = createContext<LlmSettingsContextValue | null>(null);

export function useLlmSettingsProvider(): LlmSettingsContextValue {
  const [llmSettings, setLlmSettings] = useState<ApiLLMSettings | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    let isActive = true;

    const loadSettings = async () => {
      const result = await VaultAPI.getSettings();
      if (!isActive) return;

      if (result.ok) {
        setLlmSettings(result.data.llm);
      } else {
        toast.error('Failed to load chat settings', {
          message: result.error,
        });
      }

      setIsLoading(false);
    };

    loadSettings();

    return () => {
      isActive = false;
    };
  }, []);

  const saveLlmUpdates = useCallback(
    async (updates: Partial<ApiLLMSettings>): Promise<boolean> => {
      if (!llmSettings) return false;

      const previous = llmSettings;
      const next = { ...llmSettings, ...updates };
      setLlmSettings(next);

      const result = await VaultAPI.updateSettings({
        category: 'llm',
        updates,
      });

      if (!result.ok) {
        setLlmSettings(previous);
        toast.error('Failed to update chat settings', {
          message: result.error,
        });
        return false;
      }

      return true;
    },
    [llmSettings]
  );

  return { llmSettings, isLoading, saveLlmUpdates };
}

export function useLlmSettings(): LlmSettingsContextValue {
  const ctx = useContext(LlmSettingsContext);
  if (!ctx) {
    throw new Error('useLlmSettings must be used within an LlmSettingsProvider');
  }
  return ctx;
}
