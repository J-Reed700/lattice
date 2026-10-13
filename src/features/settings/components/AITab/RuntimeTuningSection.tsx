import type { Dispatch, SetStateAction } from 'react';

import { SettingsRow, SettingsSection, Switch } from '@/components/ui';
import type { LLMSettings as ApiLLMSettings } from '@/types/api/settings';

import { SWITCH_CLASS, NUMBER_FIELD_CLASS, toFinite, clamp } from './shared';


/** Mirrors MIN/MAX_LLM_STALL_TIMEOUT_SECONDS in the settings repository. */
const STALL_TIMEOUT_MIN = 15;
const STALL_TIMEOUT_MAX = 180;

// Bounds for an explicit local context window. The floor is a window that can
// still hold a turn; the ceiling is the largest any current model is trained
// for, so a typo cannot ask for something no model supports.
const LOCAL_CONTEXT_WINDOW_MIN = 512;
const LOCAL_CONTEXT_WINDOW_MAX = 262144;
// What switching off Auto starts from — the previous fixed default, so the
// first thing a user sees is the behaviour they had before.
const LOCAL_CONTEXT_WINDOW_FALLBACK = 8192;

/** The model-runtime values a field may hold while it is being edited. */
export type RuntimeDraft = {
  temperature: number;
  topP: number;
  topK: number;
  repeatPenalty: number;
  presencePenalty: number;
  maxTokens: number;
  contextWindow: number;
  localContextWindow: number | null;
  timeoutSeconds: number;
};

/** Sampling, context and timeout settings for the chat model. */
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

      <SettingsRow label="Max tokens" htmlFor="tuning-max-tokens">
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

      <SettingsRow label="Context window" htmlFor="tuning-context-window">
        <input
          id="tuning-context-window"
          type="number"
          min={512}
          max={262144}
          step={1}
          value={runtimeDraft.contextWindow}
          onChange={(event) =>
            setRuntimeDraft((previous) => ({
              ...previous,
              contextWindow: Math.round(toFinite(event.target.value, previous.contextWindow)),
            }))
          }
          onBlur={() =>
            void saveLlmUpdates({
              contextWindow: Math.round(clamp(runtimeDraft.contextWindow, 512, 262144)),
              maxTokens: Math.round(
                clamp(runtimeDraft.maxTokens, 128, runtimeDraft.contextWindow)
              ),
            })
          }
          className={NUMBER_FIELD_CLASS}
        />
      </SettingsRow>

      <SettingsRow
        label="Local model context window"
        hint="Auto sizes the window from what the model was trained for and how much memory your GPU reports. Override it only if you know you want a different one — a window larger than the model was trained for is stretched by rope scaling, which costs quality, and one too large for the card falls back to CPU."
        htmlFor="tuning-local-context-window"
      >
        <div className="flex items-center gap-3">
          <label className="flex items-center gap-1.5 whitespace-nowrap text-sm">
            <input
              type="checkbox"
              checked={runtimeDraft.localContextWindow === null}
              onChange={(event) => {
                const next = event.target.checked ? null : LOCAL_CONTEXT_WINDOW_FALLBACK;
                setRuntimeDraft((previous) => ({ ...previous, localContextWindow: next }));
                void saveLlmUpdates({ localContextWindow: next });
              }}
            />
            Auto
          </label>
          <input
            id="tuning-local-context-window"
            type="number"
            min={LOCAL_CONTEXT_WINDOW_MIN}
            max={LOCAL_CONTEXT_WINDOW_MAX}
            step={1}
            disabled={runtimeDraft.localContextWindow === null}
            value={runtimeDraft.localContextWindow ?? ''}
            onChange={(event) =>
              setRuntimeDraft((previous) => ({
                ...previous,
                localContextWindow: Math.round(
                  toFinite(event.target.value, previous.localContextWindow ?? LOCAL_CONTEXT_WINDOW_FALLBACK)
                ),
              }))
            }
            onBlur={() => {
              if (runtimeDraft.localContextWindow === null) return;
              const clamped = Math.round(
                clamp(runtimeDraft.localContextWindow, LOCAL_CONTEXT_WINDOW_MIN, LOCAL_CONTEXT_WINDOW_MAX)
              );
              setRuntimeDraft((previous) => ({ ...previous, localContextWindow: clamped }));
              void saveLlmUpdates({ localContextWindow: clamped });
            }}
            className={NUMBER_FIELD_CLASS}
          />
        </div>
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
