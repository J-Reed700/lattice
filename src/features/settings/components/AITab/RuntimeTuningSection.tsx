import type { Dispatch, SetStateAction } from 'react';

import { SettingsRow, SettingsSection, Switch } from '@/components/ui';
import type { LLMSettings as ApiLLMSettings } from '@/types/api/settings';

import { SWITCH_CLASS, NUMBER_FIELD_CLASS, toFinite, clamp } from './shared';

/** The model-runtime values a field may hold while it is being edited. */
export type RuntimeDraft = {
  temperature: number;
  topP: number;
  topK: number;
  repeatPenalty: number;
  presencePenalty: number;
  maxTokens: number;
  contextWindow: number;
  timeoutSeconds: number;
};

/** Sampling, answer length and timeout settings for the chat model. */
export function RuntimeTuningSection({
  llmSettings,
  runtimeDraft,
  setRuntimeDraft,
  saveLlmUpdates,
}: {
  llmSettings: ApiLLMSettings;
  runtimeDraft: RuntimeDraft;
  setRuntimeDraft: Dispatch<SetStateAction<RuntimeDraft>>;
  saveLlmUpdates: (updates: Partial<ApiLLMSettings>) => Promise<boolean>;
}) {
  return (
    <SettingsSection title="Model runtime">
      <SettingsRow label="Temperature" htmlFor="tuning-temperature">
        <input
          id="tuning-temperature"
          type="number"
          min={0}
          max={2}
          step={0.05}
          value={runtimeDraft.temperature}
          onChange={(event) =>
            setRuntimeDraft((previous) => ({
              ...previous,
              temperature: toFinite(event.target.value, previous.temperature),
            }))
          }
          onBlur={() =>
            void saveLlmUpdates({ temperature: clamp(runtimeDraft.temperature, 0, 2) })
          }
          className={NUMBER_FIELD_CLASS}
        />
      </SettingsRow>

      <SettingsRow label="Top P" htmlFor="tuning-top-p">
        <input
          id="tuning-top-p"
          type="number"
          min={0}
          max={1}
          step={0.01}
          value={runtimeDraft.topP}
          onChange={(event) =>
            setRuntimeDraft((previous) => ({
              ...previous,
              topP: toFinite(event.target.value, previous.topP),
            }))
          }
          onBlur={() => void saveLlmUpdates({ topP: clamp(runtimeDraft.topP, 0, 1) })}
          className={NUMBER_FIELD_CLASS}
        />
      </SettingsRow>

      <SettingsRow label="Top K" htmlFor="tuning-top-k">
        <input
          id="tuning-top-k"
          type="number"
          min={0}
          step={1}
          value={runtimeDraft.topK}
          onChange={(event) =>
            setRuntimeDraft((previous) => ({
              ...previous,
              topK: Math.round(toFinite(event.target.value, previous.topK)),
            }))
          }
          onBlur={() => void saveLlmUpdates({ topK: Math.round(Math.max(0, runtimeDraft.topK)) })}
          className={NUMBER_FIELD_CLASS}
        />
      </SettingsRow>

      <SettingsRow label="Repeat penalty" htmlFor="tuning-repeat-penalty">
        <input
          id="tuning-repeat-penalty"
          type="number"
          min={0.05}
          step={0.05}
          value={runtimeDraft.repeatPenalty}
          onChange={(event) =>
            setRuntimeDraft((previous) => ({
              ...previous,
              repeatPenalty: toFinite(event.target.value, previous.repeatPenalty),
            }))
          }
          onBlur={() =>
            void saveLlmUpdates({ repeatPenalty: Math.max(0.05, runtimeDraft.repeatPenalty) })
          }
          className={NUMBER_FIELD_CLASS}
        />
      </SettingsRow>

      <SettingsRow
        label="Presence penalty"
        hint="Discourages repeating what was just said. Qwen 3.5 recommends 1.5; lower it if answers drift into another language."
        htmlFor="tuning-presence-penalty"
      >
        <input
          id="tuning-presence-penalty"
          type="number"
          min={-2}
          max={2}
          step={0.1}
          value={runtimeDraft.presencePenalty}
          onChange={(event) =>
            setRuntimeDraft((previous) => ({
              ...previous,
              presencePenalty: toFinite(event.target.value, previous.presencePenalty),
            }))
          }
          onBlur={() =>
            void saveLlmUpdates({ presencePenalty: clamp(runtimeDraft.presencePenalty, -2, 2) })
          }
          className={NUMBER_FIELD_CLASS}
        />
      </SettingsRow>

      <SettingsRow
        label="Max tokens"
        hint="For downloaded models, and connected models that report no limit. A connected model's own limit is set under Configure in Settings → Downloaded."
        htmlFor="tuning-max-tokens"
      >
        <input
          id="tuning-max-tokens"
          type="number"
          min={1}
          max={runtimeDraft.contextWindow}
          step={1}
          value={runtimeDraft.maxTokens}
          onChange={(event) =>
            setRuntimeDraft((previous) => ({
              ...previous,
              maxTokens: Math.round(toFinite(event.target.value, previous.maxTokens)),
            }))
          }
          onBlur={() =>
            void saveLlmUpdates({
              maxTokens: Math.round(
                clamp(runtimeDraft.maxTokens, 1, runtimeDraft.contextWindow)
              ),
            })
          }
          className={NUMBER_FIELD_CLASS}
        />
      </SettingsRow>

      <SettingsRow
        label="Stall timeout"
        hint="Seconds a response may go silent before it's treated as dead. This does not limit how long an answer may take."
        htmlFor="tuning-timeout"
      >
        <input
          id="tuning-timeout"
          type="number"
          min={1}
          step={1}
          value={runtimeDraft.timeoutSeconds}
          onChange={(event) =>
            setRuntimeDraft((previous) => ({
              ...previous,
              timeoutSeconds: Math.round(toFinite(event.target.value, previous.timeoutSeconds)),
            }))
          }
          onBlur={() =>
            void saveLlmUpdates({
              timeoutSeconds: Math.round(Math.max(1, runtimeDraft.timeoutSeconds)),
            })
          }
          className={NUMBER_FIELD_CLASS}
        />
      </SettingsRow>

      <SettingsRow label="Stream responses">
        <Switch
          className={SWITCH_CLASS}
          checked={llmSettings.streamResponses}
          onCheckedChange={(checked) => void saveLlmUpdates({ streamResponses: checked })}
          aria-label="Stream responses"
        />
      </SettingsRow>
    </SettingsSection>
  );
}
