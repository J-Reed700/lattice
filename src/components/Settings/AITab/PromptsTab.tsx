/**
 * Prompts — system prompt, templates, and response verification.
 */

import { useEffect, useState } from 'react';

import { SWITCH_CLASS, TEXTAREA_CLASS } from './shared';
import { useLlmSettings } from './useLlmSettings';
import { toast } from '../../../stores/toastStore';
import { PageHeader, SettingsRow, SettingsSection, Switch } from '../../ui';

import type { LLMSettings as ApiLLMSettings } from '../../../types/api/settings';

export function PromptsTab() {
  const { llmSettings, isLoading, saveLlmUpdates } = useLlmSettings();

  const [systemPromptDraft, setSystemPromptDraft] = useState('');
  const [greetingPromptDraft, setGreetingPromptDraft] = useState('');
  const [ragPromptDraft, setRagPromptDraft] = useState('');
  const [noContextPromptDraft, setNoContextPromptDraft] = useState('');
  const [toolFollowupPromptDraft, setToolFollowupPromptDraft] = useState('');

  useEffect(() => {
    if (!llmSettings) return;
    setSystemPromptDraft(llmSettings.prompts.systemPrompt || '');
    setGreetingPromptDraft(llmSettings.prompts.greetingPromptTemplate || '');
    setRagPromptDraft(llmSettings.prompts.ragPromptTemplate || '');
    setNoContextPromptDraft(llmSettings.prompts.noContextPromptTemplate || '');
    setToolFollowupPromptDraft(llmSettings.prompts.toolFollowupPromptTemplate || '');
  }, [llmSettings]);

  const verificationEnabled = llmSettings?.verification?.enabled ?? true;

  const handleVerificationToggle = async (enabled: boolean) => {
    if (!llmSettings) return;
    if ((llmSettings.verification?.enabled ?? true) === enabled) return;

    const ok = await saveLlmUpdates({
      verification: {
        ...(llmSettings.verification || { enabled: true }),
        enabled,
      },
    });

    if (ok) {
      toast.success(enabled ? 'Response verification enabled' : 'Response verification disabled');
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
