/**
 * CloudMetaRow
 *
 * One row per configured cloud model: the chat model while a cloud provider
 * is selected, and the Utility, Learning or Verifier model when it names a cloud
 * model. The provider, model name and key are edited on the Chat settings
 * page; whether the model sees pictures is declared here, starting from the
 * provider's published default. Role assignment uses the same choices as
 * the llama.cpp and Ollama connections.
 */

import { useState } from 'react';

import { Cloud, SlidersHorizontal } from 'lucide-react';

import { IconButton } from '@/components/ui';
import { useSettingsQuery, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import type { ModelRef } from '@/lib/bindings';
import { cn } from '@/lib/utils';
import { toast } from '@/stores/toastStore';
import { CLOUD_PROVIDER_NAMES, sameModelRef } from '@/utils/chatModelSelection';

import { ConnectionRolesDialog } from './ConnectionRolesDialog';
import { LearningWindowNotice } from './LearningWindowNotice';
import { useModelRoles } from './ModelRolesContext';
import { ActiveRoleBadge, RoleToggle } from './RoleButton';
import { CONNECTION_ROLES, roleDescriptor, type ConnectionRole } from './roleConfig';
import { ModelVisionDeclaration } from '../AITab/VisionDeclarationEditor';

export function CloudMetaRow({ modelRef, roles }: { modelRef: Extract<ModelRef, { kind: 'cloud' }>; roles: string[] }) {
  const { data: settings } = useSettingsQuery();
  const { mutateAsync: updateSettings } = useUpdateSettingsMutation();
  const { assignUtilityReference } = useModelRoles();
  const [showConfigModal, setShowConfigModal] = useState(false);
  const [pendingRole, setPendingRole] = useState<ConnectionRole | null>(null);
  const held: Record<ConnectionRole, boolean> = {
    chat: settings?.llm.provider === modelRef.provider && settings.llm.model.trim() === modelRef.model,
    utility: sameModelRef(settings?.llm.utilityModel, modelRef),
    learning: sameModelRef(settings?.llm.learningModel, modelRef),
    verifier: sameModelRef(settings?.llm.verifierModel, modelRef),
  };
  const activeRoles = CONNECTION_ROLES.filter((id) => held[id]).map(roleDescriptor);

  const toggle = async (role: ConnectionRole) => {
    if (pendingRole || !settings) return;
    setPendingRole(role);
    try {
      const on = !held[role];
      switch (role) {
        case 'chat':
          await updateSettings({ category: 'llm', updates: { provider: modelRef.provider, model: modelRef.model } });
          break;
        case 'utility':
          await assignUtilityReference(on ? modelRef : null);
          break;
        case 'learning':
          await updateSettings({ category: 'llm', updates: { learningModel: on ? modelRef : null } });
          break;
        case 'verifier':
          await updateSettings({ category: 'llm', updates: { verifierModel: on ? modelRef : null } });
      }
    } catch (error) {
      toast.error(`Failed to set ${role} role`, {
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setPendingRole(null);
    }
  };

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
          {held.learning ? <LearningWindowNotice model={modelRef} savedWindow={null} /> : null}
        </div>

        <div className="flex shrink-0 flex-wrap items-center justify-end gap-1.5">
          {activeRoles.map((role) => (
            <ActiveRoleBadge key={role.id} roleId={role.id} label={role.label} />
          ))}
          <div className={cn('flex items-center gap-1.5', activeRoles.length > 0 && 'ml-1 border-l border-border-subtle pl-1.5')}>
            <IconButton
              label="Configure model"
              data-roles={CONNECTION_ROLES.join(' ')}
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
          hint="Choose which tasks this connection powers. Role changes take effect immediately."
          onClose={() => setShowConfigModal(false)}
        >
          {CONNECTION_ROLES.map((id) => (
            <RoleToggle
              key={id}
              role={roleDescriptor(id)}
              active={held[id]}
              disabled={!settings || (id === 'chat' && held.chat)}
              pending={pendingRole !== null}
              title={id === 'chat' && held.chat ? 'Selected for chat' : undefined}
              onToggle={() => void toggle(id)}
            />
          ))}
        </ConnectionRolesDialog>
      ) : null}
    </div>
  );
}
