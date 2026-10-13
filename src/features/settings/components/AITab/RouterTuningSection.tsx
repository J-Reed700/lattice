import type { Dispatch, SetStateAction } from 'react';

import { SettingsRow, SettingsSection, Switch } from '@/components/ui';
import type { LLMSettings as ApiLLMSettings } from '@/types/api/settings';

import { SWITCH_CLASS, NUMBER_FIELD_CLASS, TEXTAREA_CLASS, toFinite, clamp } from './shared';

/** The router layer that decides how follow-up questions are answered. */
export function RouterTuningSection({
  llmSettings,
  routerDraft,
  setRouterDraft,
  saveLlmUpdates,
}: {
  llmSettings: ApiLLMSettings;
  routerDraft: ApiLLMSettings['router'];
  setRouterDraft: Dispatch<SetStateAction<ApiLLMSettings['router'] | null>>;
  saveLlmUpdates: (updates: Partial<ApiLLMSettings>) => Promise<boolean>;
}) {
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

  return (
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
          className="h-8 w-[220px] rounded-sm border border-border-default bg-bg px-2.5 text-sm text-text-primary outline-hidden transition-colors duration-fast placeholder:text-text-muted focus:border-accent"
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
  );
}
