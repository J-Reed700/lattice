import { useEffect, useState } from 'react';

import { SettingsRow } from '@/components/ui';
import type { LLMSettings as ApiLLMSettings } from '@/types/api/settings';

import { NUMBER_FIELD_CLASS, clamp, toFinite } from './shared';

const MIN_WINDOW = 512;
const MAX_WINDOW = 262_144;

/** The window of every model Lattice does not load itself, whatever task it does. */
export function ContextWindowRow({
  llmSettings,
  saveLlmUpdates,
}: {
  llmSettings: ApiLLMSettings;
  saveLlmUpdates: (updates: Partial<ApiLLMSettings>) => Promise<boolean>;
}) {
  const [draft, setDraft] = useState(llmSettings.contextWindow);
  useEffect(() => { setDraft(llmSettings.contextWindow); }, [llmSettings.contextWindow]);

  const commit = () => {
    const contextWindow = Math.round(clamp(draft, MIN_WINDOW, MAX_WINDOW));
    setDraft(contextWindow);
    if (contextWindow === llmSettings.contextWindow) return;
    void saveLlmUpdates({
      contextWindow,
      maxTokens: Math.round(clamp(llmSettings.maxTokens, 128, contextWindow)),
    });
  };

  return (
    <SettingsRow
      label="Context window"
      hint="Prompts and answers are kept within this many tokens for cloud and Ollama models, whether they chat, prepare lessons or do background work, and Ollama loads its model with it. A llama-server reports its own window, which is used instead. Downloaded models load with their own window, set in Settings → Downloaded."
      htmlFor="chatContextWindow"
    >
      <input
        id="chatContextWindow"
        type="number"
        min={MIN_WINDOW}
        max={MAX_WINDOW}
        step={1}
        value={draft}
        onChange={(event) => setDraft(Math.round(toFinite(event.target.value, draft)))}
        onBlur={commit}
        className={NUMBER_FIELD_CLASS}
      />
    </SettingsRow>
  );
}
