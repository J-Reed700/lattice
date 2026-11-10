/**
 * ModelWindowControls
 *
 * The window and output ceiling a connected model runs at, up to what its
 * provider reports. The window stays at the app's default until one is chosen
 * here, because a larger one costs more on a cloud model and more memory on
 * Ollama. The output ceiling follows the provider: it costs nothing until used.
 */

import { useEffect, useState } from 'react';

import { useModelLimits } from '@/features/model/hooks/useModelLimits';
import { useSettingsQuery, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import type { ModelRef, ModelWindowDto } from '@/lib/bindings';
import { toast } from '@/stores/toastStore';
import { CLOUD_PROVIDER_NAMES, modelRefKey } from '@/utils/chatModelSelection';

import { MIN_CONTEXT_LENGTH, TokenLimitField } from './TokenLimitField';
import { clamp } from '../AITab/shared';

const MIN_OUTPUT_TOKENS = 128;

type ConnectedModelRef = Extract<ModelRef, { kind: 'cloud' | 'ollama' }>;
type Limit = keyof ModelWindowDto;

interface LimitControlProps {
  label: string;
  help: string;
  min: number;
  /** What the provider reports, if it does. */
  reported: number | null;
  /** The value in force while nothing is chosen; null when nothing is sent. */
  following: number | null;
  chosen: number | null;
  /** Saves a value, or null to follow the provider again. */
  onSave: (value: number | null) => void;
  followLabel: string;
  placeholder?: string;
}

function LimitControl({ label, help, min, reported, following, chosen, onSave, followLabel, placeholder }: LimitControlProps) {
  const inForce = chosen ?? following;
  const [draft, setDraft] = useState<number | null>(inForce);
  useEffect(() => { setDraft(inForce); }, [inForce]);

  const commit = () => {
    if (draft === null) {
      if (chosen !== null) onSave(null);
      else setDraft(inForce);
      return;
    }
    const value = Math.round(clamp(draft, min, reported ?? Number.MAX_SAFE_INTEGER));
    setDraft(value);
    if (value !== inForce) onSave(value);
  };

  return (
    <TokenLimitField label={label} help={help} min={min} max={reported} value={draft} placeholder={placeholder} onChange={setDraft} onClear={() => setDraft(null)} onCommit={commit}>
      {chosen !== null ? (
        <button
          type="button"
          onClick={() => onSave(null)}
          className="mt-1.5 text-[11px] text-text-secondary underline underline-offset-2 hover:text-text-primary"
        >
          {followLabel}
        </button>
      ) : null}
    </TokenLimitField>
  );
}

export function ModelWindowControls({ modelRef, modelName }: { modelRef: ConnectedModelRef; modelName?: string }) {
  const { data: settings } = useSettingsQuery();
  const { mutateAsync: updateSettings } = useUpdateSettingsMutation();
  const limits = useModelLimits(modelRef);
  const key = modelRefKey(modelRef);
  const llm = settings?.llm;
  if (!llm || limits.isLoading) return null;

  const isCloud = modelRef.kind === 'cloud';
  const provider = isCloud ? CLOUD_PROVIDER_NAMES[modelRef.provider] : 'Ollama';
  const chosen: ModelWindowDto = llm.modelWindows[key] ?? { contextWindow: null, maxOutputTokens: null };
  const reportedWindow = limits.data?.contextWindow ?? null;
  const reportedOutput = limits.data?.maxOutputTokens ?? null;
  const followingWindow = Math.min(llm.contextWindow, reportedWindow ?? llm.contextWindow);

  const save = async (limit: Limit, value: number | null) => {
    const next = { ...chosen, [limit]: value };
    const { [key]: _replaced, ...others } = llm.modelWindows;
    const modelWindows = next.contextWindow === null && next.maxOutputTokens === null
      ? others
      : { ...others, [key]: next };
    try {
      await updateSettings({ category: 'llm', updates: { modelWindows } });
    } catch (error) {
      toast.error('Failed to save the model\'s limits', {
        message: error instanceof Error ? error.message : String(error),
      });
    }
  };

  return (
    <div className="space-y-4">
      {modelName ? <div className="truncate font-mono text-xs text-text-muted">{modelName}</div> : null}
      <LimitControl
        label="Context window"
        help={isCloud
          ? 'How much of a conversation and its sources this model sees at once. A long conversation sends more of itself as this grows, and cloud models bill by the token.'
          : 'How much of a conversation and its sources this model sees at once. Ollama loads the model with this window, so a larger one needs more memory.'}
        min={MIN_CONTEXT_LENGTH}
        reported={reportedWindow}
        following={followingWindow}
        chosen={chosen.contextWindow}
        onSave={(value) => void save('contextWindow', value)}
        followLabel="Use the default"
      />
      {isCloud ? (
        <LimitControl
          label="Max output"
          help="The most this model may write in one reply, thinking included. It is a ceiling, not a target: a short answer costs the same either way."
          min={MIN_OUTPUT_TOKENS}
          reported={reportedOutput}
          following={reportedOutput}
          placeholder={limits.isError ? 'Not known' : "Model's own limit"}
          chosen={chosen.maxOutputTokens}
          onSave={(value) => void save('maxOutputTokens', value)}
          followLabel={reportedOutput !== null ? `Use the maximum ${provider} reports` : 'Use the model\'s own limit'}
        />
      ) : null}
      {limits.isError || reportedWindow === null ? (
        <p role="status" className="text-[11px] text-text-muted">
          {provider} {limits.isError ? 'could not be asked for' : 'does not report'} this model&apos;s limits, so these are not held to them.
        </p>
      ) : null}
    </div>
  );
}
