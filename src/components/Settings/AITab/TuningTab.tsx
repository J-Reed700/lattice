/**
 * Tuning — model runtime parameters, the router layer, and tool output shaping.
 */

import { useEffect, useState } from 'react';

import { SWITCH_CLASS, NUMBER_FIELD_CLASS, TEXTAREA_CLASS, toFinite, clamp } from './shared';
import { useLlmSettings } from './useLlmSettings';
import { PageHeader, SettingsRow, SettingsSection, Switch } from '../../ui';

import type { LLMSettings as ApiLLMSettings } from '../../../types/api/settings';

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

export function TuningTab() {
  const { llmSettings, isLoading, saveLlmUpdates } = useLlmSettings();

  const [runtimeDraft, setRuntimeDraft] = useState({
    temperature: 0.7,
    topP: 0.9,
    topK: 40,
    repeatPenalty: 1.1,
    maxTokens: 131072,
    contextWindow: 131072,
    localContextWindow: null as number | null,
    timeoutSeconds: 30,
  });
  const [routerDraft, setRouterDraft] = useState<ApiLLMSettings['router'] | null>(null);
  const [toolOutputDraft, setToolOutputDraft] = useState<ApiLLMSettings['toolOutput'] | null>(null);

  useEffect(() => {
    if (!llmSettings) return;
    setRuntimeDraft({
      temperature: llmSettings.temperature,
      topP: llmSettings.topP,
      topK: llmSettings.topK,
      repeatPenalty: llmSettings.repeatPenalty,
      maxTokens: llmSettings.maxTokens,
      contextWindow: llmSettings.contextWindow,
      localContextWindow: llmSettings.localContextWindow,
      timeoutSeconds: llmSettings.timeoutSeconds,
    });
    setRouterDraft(llmSettings.router);
    setToolOutputDraft(llmSettings.toolOutput);
  }, [llmSettings]);

  const saveRouterField = async <K extends keyof ApiLLMSettings['router']>(
    key: K,
    value: ApiLLMSettings['router'][K]
  ) => {
    if (!llmSettings) return;
    if (llmSettings.router[key] === value) return;
    await saveLlmUpdates({
      router: {
        ...llmSettings.router,
        [key]: value,
      },
    });
  };

  const saveToolOutputField = async <K extends keyof ApiLLMSettings['toolOutput']>(
    key: K,
    value: ApiLLMSettings['toolOutput'][K]
  ) => {
    if (!llmSettings) return;
    if (llmSettings.toolOutput[key] === value) return;
    await saveLlmUpdates({
      toolOutput: {
        ...llmSettings.toolOutput,
        [key]: value,
      },
    });
  };

  const saveToolTemplateField = async <K extends keyof ApiLLMSettings['toolOutput']['templates']>(
    key: K,
    value: ApiLLMSettings['toolOutput']['templates'][K]
  ) => {
    if (!llmSettings) return;
    if (llmSettings.toolOutput.templates[key] === value) return;
    await saveLlmUpdates({
      toolOutput: {
        ...llmSettings.toolOutput,
        templates: {
          ...llmSettings.toolOutput.templates,
          [key]: value,
        },
      },
    });
  };

  if (isLoading || !llmSettings || !routerDraft || !toolOutputDraft) {
    return (
      <>
        <PageHeader title="Tuning" />
        <p className="text-sm text-text-muted">Loading…</p>
      </>
    );
  }

  return (
    <>
      <PageHeader title="Tuning" />

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

      <SettingsSection title="Router">
        <SettingsRow label="Route follow-up questions">
          <Switch
            className={SWITCH_CLASS}
            checked={routerDraft.enabled}
            onCheckedChange={(checked) => {
              setRouterDraft((previous) => (previous ? { ...previous, enabled: checked } : previous));
              void saveRouterField('enabled', checked);
            }}
            aria-label="Route follow-up questions"
          />
        </SettingsRow>

        <SettingsRow label="Prefer the last document">
          <Switch
            className={SWITCH_CLASS}
            checked={routerDraft.preferLastDocument}
            onCheckedChange={(checked) => {
              setRouterDraft((previous) =>
                previous ? { ...previous, preferLastDocument: checked } : previous
              );
              void saveRouterField('preferLastDocument', checked);
            }}
            aria-label="Prefer the last document"
          />
        </SettingsRow>

        <SettingsRow label="Router model" htmlFor="router-model">
          <input
            id="router-model"
            type="text"
            value={routerDraft.model}
            onChange={(event) =>
              setRouterDraft((previous) =>
                previous ? { ...previous, model: event.target.value } : previous
              )
            }
            onBlur={() => void saveRouterField('model', routerDraft.model.trim())}
            className="h-8 w-[220px] rounded-sm border border-border-default bg-bg px-2.5 text-sm text-text-primary outline-none transition-colors duration-fast placeholder:text-text-muted focus:border-accent"
          />
        </SettingsRow>

        <SettingsRow label="Timeout" hint="Milliseconds" htmlFor="router-timeout">
          <input
            id="router-timeout"
            type="number"
            min={50}
            max={5000}
            step={10}
            value={routerDraft.timeoutMs}
            onChange={(event) =>
              setRouterDraft((previous) =>
                previous
                  ? {
                      ...previous,
                      timeoutMs: Math.round(toFinite(event.target.value, previous.timeoutMs)),
                    }
                  : previous
              )
            }
            onBlur={() =>
              void saveRouterField('timeoutMs', Math.round(clamp(routerDraft.timeoutMs, 50, 5000)))
            }
            className={NUMBER_FIELD_CLASS}
          />
        </SettingsRow>

        <SettingsRow label="Max tokens" htmlFor="router-max-tokens">
          <input
            id="router-max-tokens"
            type="number"
            min={16}
            max={1024}
            step={1}
            value={routerDraft.maxTokens}
            onChange={(event) =>
              setRouterDraft((previous) =>
                previous
                  ? {
                      ...previous,
                      maxTokens: Math.round(toFinite(event.target.value, previous.maxTokens)),
                    }
                  : previous
              )
            }
            onBlur={() =>
              void saveRouterField('maxTokens', Math.round(clamp(routerDraft.maxTokens, 16, 1024)))
            }
            className={NUMBER_FIELD_CLASS}
          />
        </SettingsRow>

        <SettingsRow label="Temperature" htmlFor="router-temperature">
          <input
            id="router-temperature"
            type="number"
            min={0}
            max={1}
            step={0.01}
            value={routerDraft.temperature}
            onChange={(event) =>
              setRouterDraft((previous) =>
                previous
                  ? { ...previous, temperature: toFinite(event.target.value, previous.temperature) }
                  : previous
              )
            }
            onBlur={() =>
              void saveRouterField('temperature', clamp(routerDraft.temperature, 0, 1))
            }
            className={NUMBER_FIELD_CLASS}
          />
        </SettingsRow>

        <SettingsRow label="Ambiguity threshold" htmlFor="router-ambiguity">
          <input
            id="router-ambiguity"
            type="number"
            min={0}
            max={1}
            step={0.01}
            value={routerDraft.ambiguityThreshold}
            onChange={(event) =>
              setRouterDraft((previous) =>
                previous
                  ? {
                      ...previous,
                      ambiguityThreshold: toFinite(
                        event.target.value,
                        previous.ambiguityThreshold
                      ),
                    }
                  : previous
              )
            }
            onBlur={() =>
              void saveRouterField(
                'ambiguityThreshold',
                clamp(routerDraft.ambiguityThreshold, 0, 1)
              )
            }
            className={NUMBER_FIELD_CLASS}
          />
        </SettingsRow>

        <SettingsRow label="Router prompt" htmlFor="router-prompt" stacked>
          <textarea
            id="router-prompt"
            value={routerDraft.promptTemplate}
            onChange={(event) =>
              setRouterDraft((previous) =>
                previous ? { ...previous, promptTemplate: event.target.value } : previous
              )
            }
            onBlur={() => void saveRouterField('promptTemplate', routerDraft.promptTemplate)}
            rows={6}
            className={TEXTAREA_CLASS}
          />
        </SettingsRow>

        <SettingsRow label="Clarifying question prompt" htmlFor="router-clarify" stacked>
          <textarea
            id="router-clarify"
            value={routerDraft.clarifyPromptTemplate}
            onChange={(event) =>
              setRouterDraft((previous) =>
                previous ? { ...previous, clarifyPromptTemplate: event.target.value } : previous
              )
            }
            onBlur={() =>
              void saveRouterField('clarifyPromptTemplate', routerDraft.clarifyPromptTemplate)
            }
            rows={3}
            className={TEXTAREA_CLASS}
          />
        </SettingsRow>
      </SettingsSection>

      <SettingsSection title="Tool output">
        <SettingsRow label="Max characters" htmlFor="tool-max-chars">
          <input
            id="tool-max-chars"
            type="number"
            min={200}
            max={200000}
            step={50}
            value={toolOutputDraft.maxChars}
            onChange={(event) =>
              setToolOutputDraft((previous) =>
                previous
                  ? {
                      ...previous,
                      maxChars: Math.round(toFinite(event.target.value, previous.maxChars)),
                    }
                  : previous
              )
            }
            onBlur={() =>
              void saveToolOutputField(
                'maxChars',
                Math.round(clamp(toolOutputDraft.maxChars, 200, 200000))
              )
            }
            className={NUMBER_FIELD_CLASS}
          />
        </SettingsRow>

        <SettingsRow label="Excerpt characters" htmlFor="tool-excerpt-chars">
          <input
            id="tool-excerpt-chars"
            type="number"
            min={100}
            max={4000}
            step={50}
            value={toolOutputDraft.excerptChars}
            onChange={(event) =>
              setToolOutputDraft((previous) =>
                previous
                  ? {
                      ...previous,
                      excerptChars: Math.round(
                        toFinite(event.target.value, previous.excerptChars)
                      ),
                    }
                  : previous
              )
            }
            onBlur={() =>
              void saveToolOutputField(
                'excerptChars',
                Math.round(clamp(toolOutputDraft.excerptChars, 100, 4000))
              )
            }
            className={NUMBER_FIELD_CLASS}
          />
        </SettingsRow>

        <SettingsRow label="Max results" htmlFor="tool-max-results">
          <input
            id="tool-max-results"
            type="number"
            min={1}
            max={48}
            step={1}
            value={toolOutputDraft.maxResults}
            onChange={(event) =>
              setToolOutputDraft((previous) =>
                previous
                  ? {
                      ...previous,
                      maxResults: Math.round(toFinite(event.target.value, previous.maxResults)),
                    }
                  : previous
              )
            }
            onBlur={() =>
              void saveToolOutputField(
                'maxResults',
                Math.round(clamp(toolOutputDraft.maxResults, 1, 48))
              )
            }
            className={NUMBER_FIELD_CLASS}
          />
        </SettingsRow>

        <SettingsRow label="Highlight terms" htmlFor="tool-highlight-terms">
          <input
            id="tool-highlight-terms"
            type="number"
            min={1}
            max={32}
            step={1}
            value={toolOutputDraft.highlightTermsMax}
            onChange={(event) =>
              setToolOutputDraft((previous) =>
                previous
                  ? {
                      ...previous,
                      highlightTermsMax: Math.round(
                        toFinite(event.target.value, previous.highlightTermsMax)
                      ),
                    }
                  : previous
              )
            }
            onBlur={() =>
              void saveToolOutputField(
                'highlightTermsMax',
                Math.round(clamp(toolOutputDraft.highlightTermsMax, 1, 32))
              )
            }
            className={NUMBER_FIELD_CLASS}
          />
        </SettingsRow>

        <SettingsRow label="Default template" htmlFor="tool-default-template" stacked>
          <textarea
            id="tool-default-template"
            value={toolOutputDraft.templates.defaultTemplate}
            onChange={(event) =>
              setToolOutputDraft((previous) =>
                previous
                  ? {
                      ...previous,
                      templates: { ...previous.templates, defaultTemplate: event.target.value },
                    }
                  : previous
              )
            }
            onBlur={() =>
              void saveToolTemplateField(
                'defaultTemplate',
                toolOutputDraft.templates.defaultTemplate
              )
            }
            rows={3}
            className={TEXTAREA_CLASS}
          />
        </SettingsRow>

        <SettingsRow label="Get document template" htmlFor="tool-get-document-template" stacked>
          <textarea
            id="tool-get-document-template"
            value={toolOutputDraft.templates.getDocumentTemplate}
            onChange={(event) =>
              setToolOutputDraft((previous) =>
                previous
                  ? {
                      ...previous,
                      templates: {
                        ...previous.templates,
                        getDocumentTemplate: event.target.value,
                      },
                    }
                  : previous
              )
            }
            onBlur={() =>
              void saveToolTemplateField(
                'getDocumentTemplate',
                toolOutputDraft.templates.getDocumentTemplate
              )
            }
            rows={3}
            className={TEXTAREA_CLASS}
          />
        </SettingsRow>

        <SettingsRow label="Semantic search template" htmlFor="tool-semantic-template" stacked>
          <textarea
            id="tool-semantic-template"
            value={toolOutputDraft.templates.semanticSearchTemplate}
            onChange={(event) =>
              setToolOutputDraft((previous) =>
                previous
                  ? {
                      ...previous,
                      templates: {
                        ...previous.templates,
                        semanticSearchTemplate: event.target.value,
                      },
                    }
                  : previous
              )
            }
            onBlur={() =>
              void saveToolTemplateField(
                'semanticSearchTemplate',
                toolOutputDraft.templates.semanticSearchTemplate
              )
            }
            rows={3}
            className={TEXTAREA_CLASS}
          />
        </SettingsRow>
      </SettingsSection>
    </>
  );
}
