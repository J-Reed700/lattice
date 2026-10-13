/**
 * Thinking and decoding: whether answers think first, the sampling a thinking
 * answer runs with, and the experimental speculative decoding switch.
 */

import { useEffect, useState } from 'react';

import { SettingsRow, SettingsSection, Switch } from '@/components/ui';
import type { LLMSettings as ApiLLMSettings } from '@/types/api/settings';

import { INPUT_CLASS, SWITCH_CLASS, NUMBER_FIELD_CLASS, toFinite, clamp } from './shared';

type Profile = ApiLLMSettings['thinkingSampling'];

const PROFILE_FIELDS: {
  key: keyof Profile;
  label: string;
  id: string;
  min: number;
  max: number;
  step: number;
  integer?: boolean;
}[] = [
  { key: 'temperature', label: 'Thinking temperature', id: 'thinking-temperature', min: 0, max: 2, step: 0.05 },
  { key: 'topP', label: 'Thinking top P', id: 'thinking-top-p', min: 0, max: 1, step: 0.01 },
  { key: 'topK', label: 'Thinking top K', id: 'thinking-top-k', min: 0, max: 500, step: 1, integer: true },
  {
    key: 'presencePenalty',
    label: 'Thinking presence penalty',
    id: 'thinking-presence-penalty',
    min: -2,
    max: 2,
    step: 0.1,
  },
  {
    key: 'repeatPenalty',
    label: 'Thinking repeat penalty',
    id: 'thinking-repeat-penalty',
    min: 0.5,
    max: 2,
    step: 0.05,
  },
];

/** Mirror DEFAULT_MTP_DRAFT_N_MAX and MAX_MTP_DRAFT_N_MAX in the settings contract. */
const MTP_DRAFT_DEFAULT = 3;
const MTP_DRAFT_MAX = 8;

type Props = {
  llmSettings: ApiLLMSettings;
  saveLlmUpdates: (updates: Partial<ApiLLMSettings>) => Promise<boolean>;
};

export function ThinkingSection({ llmSettings, saveLlmUpdates }: Props) {
  const speculative = llmSettings.speculativeDecoding;
  const [profileDraft, setProfileDraft] = useState<Profile>(llmSettings.thinkingSampling);
  const [draftTokens, setDraftTokens] = useState(
    speculative.mode === 'mtpDraft' ? speculative.nMax : MTP_DRAFT_DEFAULT
  );

  useEffect(() => {
    setProfileDraft(llmSettings.thinkingSampling);
  }, [llmSettings.thinkingSampling]);
  useEffect(() => {
    if (speculative.mode === 'mtpDraft') setDraftTokens(speculative.nMax);
  }, [speculative]);

  const saveProfileField = (field: (typeof PROFILE_FIELDS)[number]) => {
    const raw = clamp(profileDraft[field.key], field.min, field.max);
    const value = field.integer ? Math.round(raw) : raw;
    setProfileDraft((previous) => ({ ...previous, [field.key]: value }));
    if (llmSettings.thinkingSampling[field.key] === value) return;
    void saveLlmUpdates({ thinkingSampling: { ...llmSettings.thinkingSampling, [field.key]: value } });
  };

  return (
    <>
      <SettingsSection title="Thinking">
        <SettingsRow
          label="Think before answering"
          hint="Lets the model reason before an answer that uses no tools. Rounds that can call tools never think: Qwen 3.5 9B writes its tool calls inside its thinking, where they are lost. Thinking answers are slower and use the sampling below."
        >
          <Switch
            className={SWITCH_CLASS}
            checked={llmSettings.answerThinking}
            onCheckedChange={(checked) => void saveLlmUpdates({ answerThinking: checked })}
            aria-label="Think before answering"
          />
        </SettingsRow>

        <SettingsRow
          label="Thinking for every request"
          hint="Auto follows the rule above, except for models that break without thinking (Laguna), which always think. On and Off apply to every request, tool rounds included."
          htmlFor="thinking-override"
        >
          <select
            id="thinking-override"
            value={llmSettings.thinkingOverride}
            onChange={(event) =>
              void saveLlmUpdates({
                thinkingOverride: event.target.value as ApiLLMSettings['thinkingOverride'],
              })
            }
            className={INPUT_CLASS}
          >
            <option value="auto">Auto</option>
            <option value="on">Always on</option>
            <option value="off">Always off</option>
          </select>
        </SettingsRow>

        {PROFILE_FIELDS.map((field) => (
          <SettingsRow key={field.key} label={field.label} htmlFor={field.id}>
            <input
              id={field.id}
              type="number"
              min={field.min}
              max={field.max}
              step={field.step}
              value={profileDraft[field.key]}
              onChange={(event) =>
                setProfileDraft((previous) => ({
                  ...previous,
                  [field.key]: toFinite(event.target.value, previous[field.key]),
                }))
              }
              onBlur={() => saveProfileField(field)}
              className={NUMBER_FIELD_CLASS}
            />
          </SettingsRow>
        ))}
      </SettingsSection>

      <SettingsSection title="Speculative decoding">
        <SettingsRow
          label="Draft with the model's MTP head (experimental)"
          hint="Drafts several tokens a step with a multi-token-prediction head, which only MTP builds of a model carry; other models are unaffected. The local server then answers one request at a time. Takes effect the next time the model loads."
        >
          <Switch
            className={SWITCH_CLASS}
            checked={speculative.mode === 'mtpDraft'}
            onCheckedChange={(checked) =>
              void saveLlmUpdates({
                speculativeDecoding: checked ? { mode: 'mtpDraft', nMax: draftTokens } : { mode: 'off' },
              })
            }
            aria-label="Draft with the model's MTP head"
          />
        </SettingsRow>

        <SettingsRow label="Draft tokens" hint="Tokens drafted per step." htmlFor="speculative-draft-tokens">
          <input
            id="speculative-draft-tokens"
            type="number"
            min={1}
            max={MTP_DRAFT_MAX}
            step={1}
            disabled={speculative.mode !== 'mtpDraft'}
            value={draftTokens}
            onChange={(event) =>
              setDraftTokens((previous) => Math.round(toFinite(event.target.value, previous)))
            }
            onBlur={() => {
              const nMax = Math.round(clamp(draftTokens, 1, MTP_DRAFT_MAX));
              setDraftTokens(nMax);
              if (speculative.mode !== 'mtpDraft' || speculative.nMax === nMax) return;
              void saveLlmUpdates({ speculativeDecoding: { mode: 'mtpDraft', nMax } });
            }}
            className={NUMBER_FIELD_CLASS}
          />
        </SettingsRow>
      </SettingsSection>
    </>
  );
}
