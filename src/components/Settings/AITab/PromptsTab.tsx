/**
 * Prompts — system prompt, templates, and response verification.
 */

import { useEffect, useState } from 'react';

import { NUMBER_FIELD_CLASS, SWITCH_CLASS, TEXTAREA_CLASS, clamp, toFinite } from './shared';
import { useLlmSettings } from './useLlmSettings';
import { toast } from '../../../stores/toastStore';
import { PageHeader, SettingsRow, SettingsSection, Switch } from '../../ui';

import type { LLMSettings as ApiLLMSettings } from '../../../types/api/settings';

type VerificationSettings = ApiLLMSettings['verification'];

/**
 * What the backend falls back to for a settings document written before the
 * judge had its own sampling. It exists so every write can spread the stored
 * verification block without a partial object standing in for it.
 */
const VERIFICATION_DEFAULTS: VerificationSettings = {
  enabled: true,
  temperature: 0,
  topP: 1,
  topK: 1,
};

type VerificationNumericKey = 'temperature' | 'topP' | 'topK';

type VerificationFieldConfig = {
  key: VerificationNumericKey;
  label: string;
  inputId: string;
  min: number;
  max: number;
  step: number;
};

const VERIFICATION_FIELDS: VerificationFieldConfig[] = [
  {
    key: 'temperature',
    label: 'Temperature',
    inputId: 'verification-temperature',
    min: 0,
    max: 2,
    step: 0.05,
  },
  { key: 'topP', label: 'Top P', inputId: 'verification-top-p', min: 0, max: 1, step: 0.05 },
  { key: 'topK', label: 'Top K', inputId: 'verification-top-k', min: 1, max: 500, step: 1 },
];

function normalizeVerificationValue(field: VerificationFieldConfig, value: number): number {
  const clamped = clamp(value, field.min, field.max);
  return field.step >= 1 ? Math.round(clamped) : Number(clamped.toFixed(2));
}

export function PromptsTab() {
  const { llmSettings, isLoading, saveLlmUpdates } = useLlmSettings();

  const [systemPromptDraft, setSystemPromptDraft] = useState('');
  const [greetingPromptDraft, setGreetingPromptDraft] = useState('');
  const [ragPromptDraft, setRagPromptDraft] = useState('');
  const [noContextPromptDraft, setNoContextPromptDraft] = useState('');
  const [toolFollowupPromptDraft, setToolFollowupPromptDraft] = useState('');
  const [verificationDraft, setVerificationDraft] = useState({
    temperature: VERIFICATION_DEFAULTS.temperature,
    topP: VERIFICATION_DEFAULTS.topP,
    topK: VERIFICATION_DEFAULTS.topK,
  });

  useEffect(() => {
    if (!llmSettings) return;
    setSystemPromptDraft(llmSettings.prompts.systemPrompt || '');
    setGreetingPromptDraft(llmSettings.prompts.greetingPromptTemplate || '');
    setRagPromptDraft(llmSettings.prompts.ragPromptTemplate || '');
    setNoContextPromptDraft(llmSettings.prompts.noContextPromptTemplate || '');
    setToolFollowupPromptDraft(llmSettings.prompts.toolFollowupPromptTemplate || '');
    const verification = llmSettings.verification;
    setVerificationDraft({
      temperature: verification?.temperature ?? VERIFICATION_DEFAULTS.temperature,
      topP: verification?.topP ?? VERIFICATION_DEFAULTS.topP,
      topK: verification?.topK ?? VERIFICATION_DEFAULTS.topK,
    });
  }, [llmSettings]);

  const verificationEnabled = llmSettings?.verification?.enabled ?? true;

  const handleVerificationToggle = async (enabled: boolean) => {
    if (!llmSettings) return;
    if ((llmSettings.verification?.enabled ?? true) === enabled) return;

    const ok = await saveLlmUpdates({
      verification: {
        ...(llmSettings.verification || VERIFICATION_DEFAULTS),
        enabled,
      },
    });

    if (ok) {
      toast.success(enabled ? 'Response verification enabled' : 'Response verification disabled');
    }
  };

  const saveVerificationField = async (field: VerificationFieldConfig, value: number) => {
    if (!llmSettings) return;

    const normalized = normalizeVerificationValue(field, value);
    setVerificationDraft((previous) => ({ ...previous, [field.key]: normalized }));
    if (llmSettings.verification?.[field.key] === normalized) return;

    const ok = await saveLlmUpdates({
      verification: {
        ...(llmSettings.verification || VERIFICATION_DEFAULTS),
        [field.key]: normalized,
      },
    });

    if (ok) {
      toast.success(`${field.label} saved`);
    }
  };

  const savePromptField = async (key: keyof ApiLLMSettings['prompts'], value: string) => {
    if (!llmSettings) return;
    const trimmed = value.trim();
    if (trimmed === llmSettings.prompts[key]) return;
    await saveLlmUpdates({
      prompts: {
        ...llmSettings.prompts,
        [key]: trimmed,
      },
    });
  };

  if (isLoading || !llmSettings) {
    return (
      <>
        <PageHeader title="Prompts" />
        <p className="text-sm text-text-muted">Loading…</p>
      </>
    );
  }

  const templates: Array<{
    key: keyof ApiLLMSettings['prompts'];
    label: string;
    value: string;
    setValue: (next: string) => void;
    rows: number;
  }> = [
    {
      key: 'systemPrompt',
      label: 'System prompt',
      value: systemPromptDraft,
      setValue: setSystemPromptDraft,
      rows: 4,
    },
    {
      key: 'greetingPromptTemplate',
      label: 'Greeting',
      value: greetingPromptDraft,
      setValue: setGreetingPromptDraft,
      rows: 3,
    },
    {
      key: 'ragPromptTemplate',
      label: 'With documents',
      value: ragPromptDraft,
      setValue: setRagPromptDraft,
      rows: 6,
    },
    {
      key: 'noContextPromptTemplate',
      label: 'Without documents',
      value: noContextPromptDraft,
      setValue: setNoContextPromptDraft,
      rows: 4,
    },
    {
      key: 'toolFollowupPromptTemplate',
      label: 'After a tool call',
      value: toolFollowupPromptDraft,
      setValue: setToolFollowupPromptDraft,
      rows: 4,
    },
  ];

  return (
    <>
      <PageHeader title="Prompts" />

      <SettingsSection title="Verification">
        <SettingsRow
          label="Verify responses"
          hint="Marks verified and unverified claims in assistant messages."
        >
          <Switch
            className={SWITCH_CLASS}
            checked={verificationEnabled}
            onCheckedChange={(checked) => void handleVerificationToggle(checked)}
            aria-label="Verify responses"
          />
        </SettingsRow>

        {VERIFICATION_FIELDS.map((field) => (
          <SettingsRow key={field.key} label={field.label} htmlFor={field.inputId}>
            <input
              id={field.inputId}
              type="number"
              min={field.min}
              max={field.max}
              step={field.step}
              value={verificationDraft[field.key]}
              disabled={!verificationEnabled}
              onChange={(event) =>
                setVerificationDraft((previous) => {
                  const parsed = toFinite(event.target.value, previous[field.key]);
                  return {
                    ...previous,
                    [field.key]: field.step >= 1 ? Math.round(parsed) : parsed,
                  };
                })
              }
              onBlur={(event) =>
                void saveVerificationField(
                  field,
                  toFinite(event.target.value, verificationDraft[field.key])
                )
              }
              className={NUMBER_FIELD_CLASS}
            />
          </SettingsRow>
        ))}

        <p className="py-2 text-sm text-text-muted">
          A verdict is a classification, not a piece of writing. Temperature 0 keeps the judge
          greedy, so the same claim against the same passage gets the same answer on every turn
          instead of a different one each time.
        </p>
      </SettingsSection>

      <SettingsSection title="Templates">
        {templates.map((template) => {
          const inputId = `prompt-${String(template.key)}`;
          return (
            <SettingsRow key={template.key} label={template.label} htmlFor={inputId} stacked>
              <textarea
                id={inputId}
                value={template.value}
                onChange={(e) => template.setValue(e.target.value)}
                onBlur={() => void savePromptField(template.key, template.value)}
                rows={template.rows}
                className={TEXTAREA_CLASS}
              />
            </SettingsRow>
          );
        })}
      </SettingsSection>
    </>
  );
}
