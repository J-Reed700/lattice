/**
 * CloudMetaRow
 *
 * One row per configured cloud model: the chat model while a cloud provider
 * is selected, and the Learning or Verifier model when it names a cloud
 * model. The provider, model name and key are edited on the Chat settings
 * page; whether the model sees pictures is declared here, starting from the
 * provider's published default. The Verifier is the one role a cloud model
 * may take from this row.
 */

import { useState } from 'react';

import { Check, Cloud, Plus } from 'lucide-react';

import { useSettingsQuery, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import type { ModelRef } from '@/lib/bindings';
import { cn } from '@/lib/utils';
import { CLOUD_PROVIDER_NAMES, sameModelRef } from '@/utils/chatModelSelection';

import { ROLES } from './roleConfig';
import { ModelVisionDeclaration } from '../AITab/VisionDeclarationEditor';

const VERIFIER = ROLES.find((role) => role.id === 'verifier');

/** Sets or clears this cloud model as the Verifier. */
function VerifierToggle({ modelRef }: { modelRef: ModelRef }) {
  const { data: settings } = useSettingsQuery();
  const { mutateAsync: updateSettings } = useUpdateSettingsMutation();
  const [pending, setPending] = useState(false);
  if (!VERIFIER) return null;
  const active = sameModelRef(settings?.llm?.verifierModel, modelRef);
  const toggle = async () => {
    if (pending) return;
    setPending(true);
    try {
      await updateSettings({ category: 'llm', updates: { verifierModel: active ? null : modelRef } });
    } finally {
      setPending(false);
    }
  };
  return (
    <button
      type="button"
      onClick={() => void toggle()}
      disabled={pending || !settings}
      aria-pressed={active}
      data-role="verifier"
      title={active ? 'Active verifier model. Click to stop using it.' : VERIFIER.hint}
      className={cn(
        'inline-flex h-7 shrink-0 items-center gap-1 rounded-md border px-2 text-xs transition-colors duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
        active
          ? 'border-accent/25 bg-accent-muted font-medium text-accent'
          : 'border-border-default text-text-secondary hover:bg-surface-raised hover:text-text-primary',
        pending && 'opacity-60',
      )}
    >
      {active ? <Check className="h-3 w-3" aria-hidden="true" /> : <Plus className="h-3 w-3" aria-hidden="true" />}
      {VERIFIER.label}
    </button>
  );
}

export function CloudMetaRow({ modelRef, roles }: { modelRef: Extract<ModelRef, { kind: 'cloud' }>; roles: string[] }) {
  const goToChatSettings = () => {
    window.dispatchEvent(new CustomEvent('settings:navigate-tab', { detail: { tab: 'chat' } }));
  };

  return (
    <div className="border-b border-border-subtle py-4">
      <div className="connected-model-row group flex items-center gap-3">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-raised text-accent"><Cloud className="h-4 w-4" aria-hidden="true" /></span>
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium text-text-primary">{CLOUD_PROVIDER_NAMES[modelRef.provider]} connection</div>
          <div className="truncate font-mono text-xs text-text-muted">{modelRef.model}</div>
          <p className="mt-1 wrap-break-word text-xs text-text-muted">{roles.join(' · ')}</p>
        </div>
        <VerifierToggle modelRef={modelRef} />
        <button
          type="button"
          onClick={goToChatSettings}
          className="shrink-0 text-xs text-text-secondary transition-colors duration-fast hover:text-text-primary"
        >
          Edit connection
        </button>
      </div>
      <div className="mt-3 pl-12">
        <ModelVisionDeclaration modelRef={modelRef} />
      </div>
    </div>
  );
}
