/**
 * CloudMetaRow
 *
 * One row per configured cloud model: the chat model while a cloud provider
 * is selected, and the Learning model when it names a cloud model. The
 * provider, model name and key are edited on the Chat settings page; whether
 * the model sees pictures is declared here, starting from the provider's
 * published default.
 */

import { Cloud } from 'lucide-react';

import type { ModelRef } from '@/lib/bindings';
import { CLOUD_PROVIDER_NAMES } from '@/utils/chatModelSelection';

import { ModelVisionDeclaration } from '../AITab/VisionDeclarationEditor';

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
