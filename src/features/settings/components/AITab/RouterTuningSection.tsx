import type { Dispatch, SetStateAction } from 'react';

import { Input, SettingsRow, SettingsSection, Switch, Textarea } from '@/components/ui';
import type { LLMSettings as ApiLLMSettings } from '@/types/api/settings';

import { SWITCH_CLASS, clamp } from './shared';
import { RangeStepperControl, SettingsNumberField } from '../SettingControls';

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
    value: ApiLLMSettings['router'][K],
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
              previous ? { ...previous, preferLastDocument: checked } : previous,
            );
            void saveRouterField('preferLastDocument', checked);
          }}
          aria-label="Prefer the last document"
        />
      </SettingsRow>

      <SettingsRow label="Router model" htmlFor="router-model">
        <div className="w-full max-w-[240px]">
          <Input
            id="router-model"
            type="text"
            value={routerDraft.model}
            onChange={(event) =>
              setRouterDraft((previous) =>
                previous ? { ...previous, model: event.target.value } : previous,
              )
            }
            onBlur={() => void saveRouterField('model', routerDraft.model.trim())}
          />
        </div>
      </SettingsRow>

      <SettingsRow label="Timeout" hint="Milliseconds" htmlFor="router-timeout">
        <SettingsNumberField
          id="router-timeout"
          min={1}
          step={10}
          integer
          suffix="ms"
          value={routerDraft.timeoutMs}
          onDraftChange={(next) =>
            setRouterDraft((previous) =>
              previous ? { ...previous, timeoutMs: Math.round(next) } : previous,
            )
          }
          onCommit={(next) => void saveRouterField('timeoutMs', Math.round(Math.max(1, next)))}
        />
      </SettingsRow>

      <SettingsRow label="Max tokens" htmlFor="router-max-tokens">
        <SettingsNumberField
          id="router-max-tokens"
          min={1}
          step={1}
          integer
          value={routerDraft.maxTokens}
          onDraftChange={(next) =>
            setRouterDraft((previous) =>
              previous ? { ...previous, maxTokens: Math.round(next) } : previous,
            )
          }
          onCommit={(next) => void saveRouterField('maxTokens', Math.round(Math.max(1, next)))}
        />
      </SettingsRow>

      <SettingsRow label="Temperature" htmlFor="router-temperature">
        <RangeStepperControl
          id="router-temperature"
          label="Router temperature"
          min={0}
          max={2}
          step={0.01}
          value={routerDraft.temperature}
          onDraftChange={(next) =>
            setRouterDraft((previous) =>
              previous ? { ...previous, temperature: next } : previous,
            )
          }
          onCommit={(next) => void saveRouterField('temperature', clamp(next, 0, 2))}
        />
      </SettingsRow>

      <SettingsRow label="Ambiguity threshold" htmlFor="router-ambiguity">
        <RangeStepperControl
          id="router-ambiguity"
          label="Ambiguity threshold"
          min={0}
          max={1}
          step={0.01}
          value={routerDraft.ambiguityThreshold}
          onDraftChange={(next) =>
            setRouterDraft((previous) =>
              previous ? { ...previous, ambiguityThreshold: next } : previous,
            )
          }
          onCommit={(next) => void saveRouterField('ambiguityThreshold', clamp(next, 0, 1))}
        />
      </SettingsRow>

      <SettingsRow label="Router prompt" htmlFor="router-prompt" stacked>
        <Textarea
          id="router-prompt"
          value={routerDraft.promptTemplate}
          onChange={(event) =>
            setRouterDraft((previous) =>
              previous ? { ...previous, promptTemplate: event.target.value } : previous,
            )
          }
          onBlur={() => void saveRouterField('promptTemplate', routerDraft.promptTemplate)}
          rows={6}
          className="font-mono text-xs"
        />
      </SettingsRow>

      <SettingsRow label="Clarifying question prompt" htmlFor="router-clarify" stacked>
        <Textarea
          id="router-clarify"
          value={routerDraft.clarifyPromptTemplate}
          onChange={(event) =>
            setRouterDraft((previous) =>
              previous ? { ...previous, clarifyPromptTemplate: event.target.value } : previous,
            )
          }
          onBlur={() =>
            void saveRouterField('clarifyPromptTemplate', routerDraft.clarifyPromptTemplate)
          }
          rows={3}
          className="font-mono text-xs"
        />
      </SettingsRow>
    </SettingsSection>
  );
}
