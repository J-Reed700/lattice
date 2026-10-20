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

import { Cloud, SlidersHorizontal } from 'lucide-react';

import { IconButton } from '@/components/ui';
import { useSettingsQuery, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import type { ModelRef } from '@/lib/bindings';
import { cn } from '@/lib/utils';
import { CLOUD_PROVIDER_NAMES, sameModelRef } from '@/utils/chatModelSelection';

import { ConnectionRolesDialog } from './ConnectionRolesDialog';
import { ActiveRoleBadge, RoleToggle } from './RoleButton';
import { roleDescriptor } from './roleConfig';
import { ModelVisionDeclaration } from '../AITab/VisionDeclarationEditor';

const VERIFIER = roleDescriptor('verifier');

/** Sets or clears this cloud model as the Verifier. */
function VerifierToggle({ modelRef }: { modelRef: ModelRef }) {
  const { data: settings } = useSettingsQuery();
  const { mutateAsync: updateSettings } = useUpdateSettingsMutation();
  const [pending, setPending] = useState(false);
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
    <RoleToggle
      role={VERIFIER}
      active={active}
      disabled={!settings}
      pending={pending}
      onToggle={() => void toggle()}
    />
  );
}

export function CloudMetaRow({ modelRef, roles }: { modelRef: Extract<ModelRef, { kind: 'cloud' }>; roles: string[] }) {
  const { data: settings } = useSettingsQuery();
  const [showConfigModal, setShowConfigModal] = useState(false);
  const isVerifier = sameModelRef(settings?.llm?.verifierModel, modelRef);

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

        <div className="flex shrink-0 flex-wrap items-center justify-end gap-1.5">
          {isVerifier ? <ActiveRoleBadge roleId="verifier" label="Verifier" /> : null}
          <div className={cn('flex items-center gap-1.5', isVerifier && 'ml-1 border-l border-border-subtle pl-1.5')}>
            <IconButton
              label="Configure model"
              data-roles="verifier"
              onClick={() => setShowConfigModal(true)}
            >
              <SlidersHorizontal />
            </IconButton>
            <button
              type="button"
              onClick={goToChatSettings}
              className="shrink-0 text-xs text-text-secondary transition-colors duration-fast hover:text-text-primary"
            >
              Edit connection
            </button>
          </div>
        </div>
      </div>
      <div className="mt-3 pl-12">
        <ModelVisionDeclaration modelRef={modelRef} />
      </div>

      {showConfigModal ? (
        <ConnectionRolesDialog
          title={`${CLOUD_PROVIDER_NAMES[modelRef.provider]} connection`}
          servedLabel="Connected model"
          served={modelRef.model}
          hint={VERIFIER.hint}
          onClose={() => setShowConfigModal(false)}
        >
          <VerifierToggle modelRef={modelRef} />
        </ConnectionRolesDialog>
      ) : null}
    </div>
  );
}
