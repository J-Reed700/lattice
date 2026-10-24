import { useState } from 'react';

import { Network, SlidersHorizontal } from 'lucide-react';

import { IconButton } from '@/components/ui';
import { useSettingsQuery, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import { cn } from '@/lib/utils';
import { toast } from '@/stores/toastStore';
import { LLAMACPP_SERVER_MODEL_ID, modelRefForOption, sameModelRef } from '@/utils/chatModelSelection';

import { ConnectionRolesDialog } from './ConnectionRolesDialog';
import { LearningWindowNotice } from './LearningWindowNotice';
import { useModelRoles } from './ModelRolesContext';
import { ActiveRoleBadge, RoleToggle } from './RoleButton';
import { roleDescriptor, type RoleId } from './roleConfig';
import { ModelVisionDeclaration } from '../AITab/VisionDeclarationEditor';

const NOT_CONFIGURED = 'Set a server URL and model in connection settings first';

type ConnectionRole = Exclude<RoleId, 'embedding'>;
const CONNECTION_ROLES: ReadonlyArray<ConnectionRole> = ['chat', 'utility', 'learning', 'verifier'];

export function LlamaCppMetaRow() {
  const { data: settings } = useSettingsQuery();
  const { mutateAsync: updateSettings } = useUpdateSettingsMutation();
  const { assignUtilityReference } = useModelRoles();
  const [showConfigModal, setShowConfigModal] = useState(false);
  const [pendingRole, setPendingRole] = useState<RoleId | null>(null);

  const connection = settings?.llm.llamaCpp;
  const url = connection?.url.trim() ?? '';
  const model = connection?.model.trim() ?? '';
  const isActive = settings?.llm.provider === 'llamacpp';
  const isConfigured = Boolean(url && model);
  const servedRef = settings && isConfigured
    ? modelRefForOption({ id: LLAMACPP_SERVER_MODEL_ID, backend: 'llamacpp' }, settings.llm)
    : null;
  const held: Record<ConnectionRole, boolean> = {
    chat: isActive && isConfigured,
    utility: sameModelRef(settings?.llm.utilityModel, servedRef),
    learning: sameModelRef(settings?.llm.learningModel, servedRef),
    verifier: sameModelRef(settings?.llm.verifierModel, servedRef),
  };
  const activeRoles = CONNECTION_ROLES.filter((id) => held[id]).map(roleDescriptor);

  const run = async (role: RoleId, action: () => Promise<void>) => {
    if (pendingRole) return;
    setPendingRole(role);
    try {
      await action();
    } catch (error) {
      toast.error(`Failed to set ${role} role`, {
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setPendingRole(null);
    }
  };

  // Chat switches the provider, so it turns off only by choosing another one.
  // The others toggle both ways, like the role buttons on a downloaded model.
  const toggle = (role: ConnectionRole) => run(role, async () => {
    if (!servedRef) return;
    const on = !held[role];
    switch (role) {
      case 'chat':
        await updateSettings({ category: 'llm', updates: { provider: 'llamacpp' } });
        return;
      case 'utility':
        await assignUtilityReference(on ? servedRef : null);
        return;
      case 'learning':
        await updateSettings({ category: 'llm', updates: { learningModel: on ? servedRef : null } });
        return;
      case 'verifier':
        await updateSettings({ category: 'llm', updates: { verifierModel: on ? servedRef : null } });
    }
  });

  const goToChatSettings = () => {
    window.dispatchEvent(new CustomEvent('settings:navigate-tab', { detail: { tab: 'chat' } }));
  };

  const meta = model ? `Model · ${model}` : 'No model selected';

  return (
    <div className="border-b border-border-subtle py-4">
      <div className="connected-model-row group flex items-center gap-3">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-raised text-accent"><Network className="h-4 w-4" aria-hidden="true" /></span>
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium text-text-primary">llama.cpp connection</div>
          {url ? <div className="truncate font-mono text-xs text-text-muted">{url}</div> : null}
          <p className="mt-1 wrap-break-word text-xs text-text-muted">{meta}</p>
          {settings?.llm.provider === 'auto' && isConfigured ? (
            <p className="mt-1 text-xs text-text-muted">
              Auto tries local models first. Select Chat to always use this connection.
            </p>
          ) : null}
          {held.learning ? <LearningWindowNotice model={servedRef} savedWindow={null} /> : null}
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
      {servedRef ? (
        <div className="mt-3 pl-12">
          <ModelVisionDeclaration modelRef={servedRef} />
        </div>
      ) : null}

      {showConfigModal ? (
        <ConnectionRolesDialog
          title="llama.cpp connection"
          servedLabel="Connected model"
          served={model || 'No model selected'}
          detail={url || undefined}
          hint="Choose which tasks this connection powers. Role changes take effect immediately."
          onClose={() => setShowConfigModal(false)}
        >
          {CONNECTION_ROLES.map((id) => (
            <RoleToggle
              key={id}
              role={roleDescriptor(id)}
              active={held[id]}
              disabled={!servedRef || (id === 'chat' && held.chat)}
              pending={pendingRole !== null}
              title={!isConfigured ? NOT_CONFIGURED : id === 'chat' && held.chat ? 'Selected for chat' : undefined}
              onToggle={() => void toggle(id)}
            />
          ))}
        </ConnectionRolesDialog>
      ) : null}
    </div>
  );
}
