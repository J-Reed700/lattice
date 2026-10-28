import type { Dispatch, SetStateAction } from 'react';

import { SettingsRow, SettingsSection, Switch } from '@/components/ui';
import type { LLMSettings as ApiLLMSettings } from '@/types/api/settings';

import { SWITCH_CLASS, NUMBER_FIELD_CLASS, toFinite, clamp } from './shared';


/** Mirrors MIN/MAX_LLM_STALL_TIMEOUT_SECONDS in the settings repository. */
const STALL_TIMEOUT_MIN = 15;
const STALL_TIMEOUT_MAX = 180;

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
          max={500}
          step={1}
          value={runtimeDraft.topK}
          onChange={(event) =>
            setRuntimeDraft((previous) => ({
              ...previous,
              topK: Math.round(toFinite(event.target.value, previous.topK)),
            }))
          }
          onBlur={() =>
            void saveLlmUpdates({ topK: Math.round(clamp(runtimeDraft.topK, 0, 500)) })
          }
          className={NUMBER_FIELD_CLASS}
        />
      </SettingsRow>

      <SettingsRow label="Repeat penalty" htmlFor="tuning-repeat-penalty">
        <input
          id="tuning-repeat-penalty"
          type="number"
          min={0.5}
          max={2}
          step={0.05}
          value={runtimeDraft.repeatPenalty}
          onChange={(event) =>
            setRuntimeDraft((previous) => ({
              ...previous,
              repeatPenalty: toFinite(event.target.value, previous.repeatPenalty),
            }))
          }
          onBlur={() =>
            void saveLlmUpdates({ repeatPenalty: clamp(runtimeDraft.repeatPenalty, 0.5, 2) })
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
        hint="At most the chat provider's context window, set in Settings → Chat."
        htmlFor="tuning-max-tokens"
      >
        <input
          id="tuning-max-tokens"
          type="number"
          min={128}
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
                clamp(runtimeDraft.maxTokens, 128, runtimeDraft.contextWindow)
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
          min={STALL_TIMEOUT_MIN}
          max={STALL_TIMEOUT_MAX}
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
              timeoutSeconds: Math.round(
                clamp(runtimeDraft.timeoutSeconds, STALL_TIMEOUT_MIN, STALL_TIMEOUT_MAX)
              ),
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
