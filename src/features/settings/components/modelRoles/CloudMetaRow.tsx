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

import * as Dialog from '@radix-ui/react-dialog';
import { Check, ChevronLeft, Cloud, Plus, SlidersHorizontal } from 'lucide-react';

import { IconButton } from '@/components/ui';
import { useSettingsQuery, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import type { ModelRef } from '@/lib/bindings';
import { cn } from '@/lib/utils';
import { CLOUD_PROVIDER_NAMES, sameModelRef } from '@/utils/chatModelSelection';

import { ActiveRoleBadge } from './RoleButton';
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
        <Dialog.Root open={showConfigModal} onOpenChange={(open) => !open && setShowConfigModal(false)}>
          <Dialog.Portal>
            <Dialog.Overlay className="fixed inset-0 z-50 bg-black/60 backdrop-blur-[2px]" />
            <Dialog.Content
              aria-describedby={undefined}
              className="fixed left-1/2 top-1/2 z-50 w-full max-w-[520px] -translate-x-1/2 -translate-y-1/2 rounded-xl border border-border-subtle bg-surface-raised shadow-2xl outline-none"
            >
              <div className="relative flex items-center justify-center border-b border-border-subtle px-5 py-3.5">
                <button
                  type="button"
                  onClick={() => setShowConfigModal(false)}
                  aria-label="Close"
                  className="absolute left-4 flex h-7 w-7 items-center justify-center rounded-md text-text-secondary transition-colors hover:bg-surface hover:text-text-primary"
                >
                  <ChevronLeft className="h-4 w-4" />
                </button>
                <Dialog.Title className="truncate text-sm font-semibold text-text-primary">
                  {CLOUD_PROVIDER_NAMES[modelRef.provider]} connection
                </Dialog.Title>
              </div>

              <div className="space-y-5 px-6 py-5">
                <div>
                  <div className="mb-1.5 text-xs font-medium text-text-secondary">Connected model</div>
                  <div className="flex flex-col gap-0.5 rounded-lg border border-border-subtle bg-surface px-3.5 py-2.5">
                    <span className="truncate text-sm font-medium text-text-primary">{modelRef.model}</span>
                    <span className="truncate text-xs text-text-muted">{roles.join(' · ')}</span>
                  </div>
                </div>

                <div>
                  <div className="mb-1.5 text-xs font-medium text-text-secondary">Roles</div>
                  <div className="flex flex-wrap items-center gap-1.5">
                    <VerifierToggle modelRef={modelRef} />
                  </div>
                  <p className="mt-1.5 text-[11px] text-text-muted">
                    Audits every claim of a lesson against its cited passages before it publishes.
                  </p>
                </div>
              </div>

              <div className="flex items-center justify-end gap-2 border-t border-border-subtle px-6 py-4">
                <button
                  type="button"
                  onClick={() => setShowConfigModal(false)}
                  className="rounded-lg bg-accent px-4 py-2 text-xs font-semibold text-white shadow-xs transition-opacity hover:opacity-90"
                >
                  Done
                </button>
              </div>
            </Dialog.Content>
          </Dialog.Portal>
        </Dialog.Root>
      ) : null}
    </div>
  );
}
