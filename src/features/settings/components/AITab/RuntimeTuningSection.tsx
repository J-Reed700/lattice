import type { Dispatch, SetStateAction } from 'react';

import { SettingsRow, SettingsSection, Switch } from '@/components/ui';
import type { LLMSettings as ApiLLMSettings } from '@/types/api/settings';

import { SWITCH_CLASS, clamp } from './shared';
import { RangeStepperControl, SettingsNumberField } from '../SettingControls';

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
      <SettingsRow
        label="Temperature"
        hint="Lower values stay focused; higher values vary wording."
        htmlFor="tuning-temperature"
      >
        <RangeStepperControl
          id="tuning-temperature"
          label="Temperature"
          min={0}
          max={2}
          step={0.05}
          value={runtimeDraft.temperature}
          onDraftChange={(next) =>
            setRuntimeDraft((previous) => ({ ...previous, temperature: next }))
          }
          onCommit={(next) => void saveLlmUpdates({ temperature: clamp(next, 0, 2) })}
        />
      </SettingsRow>

      <SettingsRow label="Top P" htmlFor="tuning-top-p">
        <RangeStepperControl
          id="tuning-top-p"
          label="Top P"
          min={0}
          max={1}
          step={0.01}
          value={runtimeDraft.topP}
          onDraftChange={(next) =>
            setRuntimeDraft((previous) => ({ ...previous, topP: next }))
          }
          onCommit={(next) => void saveLlmUpdates({ topP: clamp(next, 0, 1) })}
        />
      </SettingsRow>

      <SettingsRow label="Top K" htmlFor="tuning-top-k">
        <SettingsNumberField
          id="tuning-top-k"
          min={0}
          step={1}
          integer
          value={runtimeDraft.topK}
          onDraftChange={(next) =>
            setRuntimeDraft((previous) => ({ ...previous, topK: Math.round(next) }))
          }
          onCommit={(next) => void saveLlmUpdates({ topK: Math.round(Math.max(0, next)) })}
        />
      </SettingsRow>

      <SettingsRow label="Repeat penalty" htmlFor="tuning-repeat-penalty">
        <RangeStepperControl
          id="tuning-repeat-penalty"
          label="Repeat penalty"
          min={0.05}
          max={2}
          sliderMax={2}
          step={0.05}
          value={runtimeDraft.repeatPenalty}
          onDraftChange={(next) =>
            setRuntimeDraft((previous) => ({ ...previous, repeatPenalty: next }))
          }
          onCommit={(next) => void saveLlmUpdates({ repeatPenalty: Math.max(0.05, next) })}
        />
      </SettingsRow>

      <SettingsRow
        label="Presence penalty"
        hint="Discourages repeating what was just said. Qwen 3.5 recommends 1.5; lower it if answers drift into another language."
        htmlFor="tuning-presence-penalty"
      >
        <RangeStepperControl
          id="tuning-presence-penalty"
          label="Presence penalty"
          min={-2}
          max={2}
          step={0.1}
          value={runtimeDraft.presencePenalty}
          onDraftChange={(next) =>
            setRuntimeDraft((previous) => ({ ...previous, presencePenalty: next }))
          }
          onCommit={(next) => void saveLlmUpdates({ presencePenalty: clamp(next, -2, 2) })}
        />
      </SettingsRow>

      <SettingsRow
        label="Max tokens"
        hint="For downloaded models and models on your own llama.cpp or Ollama server. A cloud model writes up to its provider's limit unless one is set under Configure in Settings → Downloaded."
        htmlFor="tuning-max-tokens"
      >
        <SettingsNumberField
          id="tuning-max-tokens"
          min={1}
          max={runtimeDraft.contextWindow}
          step={1}
          integer
          inputClassName="w-16"
          value={runtimeDraft.maxTokens}
          onDraftChange={(next) =>
            setRuntimeDraft((previous) => ({ ...previous, maxTokens: Math.round(next) }))
          }
          onCommit={(next) =>
            void saveLlmUpdates({
              maxTokens: Math.round(clamp(next, 1, runtimeDraft.contextWindow)),
            })
          }
        />
      </SettingsRow>

      <SettingsRow
        label="Stall timeout"
        hint="Seconds a response may go silent before it's treated as dead. This does not limit how long an answer may take."
        htmlFor="tuning-timeout"
      >
        <SettingsNumberField
          id="tuning-timeout"
          min={1}
          step={1}
          integer
          suffix="s"
          value={runtimeDraft.timeoutSeconds}
          onDraftChange={(next) =>
            setRuntimeDraft((previous) => ({ ...previous, timeoutSeconds: Math.round(next) }))
          }
          onCommit={(next) =>
            void saveLlmUpdates({ timeoutSeconds: Math.round(Math.max(1, next)) })
          }
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
