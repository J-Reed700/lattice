/**
 * OllamaMetaRow
 *
 * One row representing the Ollama server endpoint, backed by a single
 * synthetic row in the `models` table (`model_id = '__ollama_server__'`,
 * `backend = 'ollama'`). Toggling Chat or Utility here flips the same
 * `is_active_for_*` flags used by local models, so the SQLite single-active
 * triggers give us mutual exclusion for free. Learning is saved in Settings
 * as the chat tag on this connection.
 *
 * Model tags (chat / utility) live in LLM settings, not on the row — many
 * hosted Ollama deployments block `/api/tags` enumeration, so users type the
 * tags manually on the Chat settings page. This row shows them read-only,
 * with a vision declaration for each tag that can answer a chat.
 */

import { useState } from 'react';

import { Network, SlidersHorizontal } from 'lucide-react';

import { IconButton } from '@/components/ui';
import { useSettingsQuery } from '@/features/settings/hooks/useSettingsQuery';
import type { ActiveModels, ModelRef } from '@/lib/bindings';
import { cn } from '@/lib/utils';
import type { LLMSettings } from '@/types/api/settings';
import type { DownloadedModel } from '@/types/downloadedModels';
import {
  CLOUD_PROVIDER_NAMES,
  isCloudProvider,
  modelRefLabel,
  remoteOption,
  sameModelRef,
} from '@/utils/chatModelSelection';

import { ConnectionRolesDialog } from './ConnectionRolesDialog';
import { LearningWindowNotice } from './LearningWindowNotice';
import { useModelRoles } from './ModelRolesContext';
import { ModelWindowControls } from './ModelWindowControls';
import { ActiveRoleBadge, RoleButton, RoleToggle } from './RoleButton';
import { CONNECTION_ROLES, isLearningModel, isUtilityModel, roleDescriptor } from './roleConfig';
import { ModelVisionDeclaration } from '../AITab/VisionDeclarationEditor';

const OLLAMA_SERVER_MODEL_ID = '__ollama_server__';

function providerName(provider: LLMSettings['provider'] | undefined): string {
  if (!provider) return '';
  if (provider === 'llamacpp') return 'llama.cpp';
  if (isCloudProvider(provider)) return CLOUD_PROVIDER_NAMES[provider];
  return provider.charAt(0).toUpperCase() + provider.slice(1);
}

function utilityNote(
  active: ActiveModels,
  ollamaRef: ModelRef | null,
  settings: LLMSettings | undefined,
  models: DownloadedModel[],
): string | null {
  const [first] = active.utility;
  if (!first) {
    return `Utility work does not run on this connection while the chat provider is ${providerName(settings?.provider)}.`;
  }
  if (sameModelRef(first, ollamaRef)) return null;
  const firstLabel = first.kind === 'local' ? modelRefLabel(first, models) : remoteOption(first, settings).triggerLabel;
  return active.utility.slice(1).some((ref) => sameModelRef(ref, ollamaRef))
    ? `Utility work tries ${firstLabel} first, then this connection.`
    : `Utility work runs on ${firstLabel} instead of this connection.`;
}

export function OllamaMetaRow() {
  const { models, isLoading, activeModels } = useModelRoles();
  // The connection and tags live in LLM settings, and the settings repository
  // is the source of truth for them. Read-only here; Chat settings is where
  // they are edited.
  const { data: settings } = useSettingsQuery();
  const [showConfigModal, setShowConfigModal] = useState(false);

  const ollamaRow = models.find((m) => m.model_id === OLLAMA_SERVER_MODEL_ID);

  if (isLoading || !ollamaRow) {
    return null;
  }

  const ollamaUrl = settings?.llm.ollamaUrl.trim() ?? '';
  const chatTag = settings?.llm.model.trim() ?? '';
  const utilityTag = activeModels?.ollama_utility_tag ?? '';
  const utilityStatus =
    activeModels && isUtilityModel(ollamaRow, settings?.llm)
      ? utilityNote(activeModels, utilityTag ? { kind: 'ollama', model: utilityTag } : null, settings?.llm, models)
      : null;

  // Learning names the chat tag on this connection: `Ollama{tag}` in Settings.
  const ollamaRoles = CONNECTION_ROLES.map(roleDescriptor);
  const activeOllamaRoles = ollamaRoles.filter((role) => role.isActive(ollamaRow, settings?.llm, activeModels));

  const goToChatSettings = () => {
    window.dispatchEvent(new CustomEvent('settings:navigate-tab', { detail: { tab: 'chat' } }));
  };

  // Each Ollama model that can answer a chat carries its own vision
  // declaration and window: the chat tag, then any tag downloaded as a row of
  // its own.
  const visionTags = Array.from(new Set([
    chatTag,
    ...models
      .filter((m) => m.backend === 'ollama' && m.model_id !== OLLAMA_SERVER_MODEL_ID
        && (m.model_type === 'language_model' || m.model_type === 'chat'))
      .map((m) => m.model_id),
  ].filter(Boolean)));

  const meta = [
    chatTag ? `Chat · ${chatTag}` : 'Chat tag not set',
    utilityTag ? `Utility · ${utilityTag}` : 'Utility tag not set',
  ].join(' · ');

  return (
    <div className="border-b border-border-subtle py-4">
      <div className="connected-model-row group flex items-center gap-3">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-raised text-accent"><Network className="h-4 w-4" aria-hidden="true" /></span>
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium text-text-primary">Ollama connection</div>
          {ollamaUrl ? (
            <div className="truncate font-mono text-xs text-text-muted">{ollamaUrl}</div>
          ) : (
            <button
              type="button"
              onClick={goToChatSettings}
              className="text-xs text-text-secondary transition-colors duration-fast hover:text-text-primary"
            >
              Set a server URL
            </button>
          )}
          <p className="mt-1 wrap-break-word text-xs text-text-muted">{meta}</p>
          {utilityStatus && (
            <div role="status" className="mt-2 max-w-xl text-xs text-text-secondary">
              {utilityStatus}{' '}
              <button type="button" onClick={goToChatSettings} className="text-accent underline underline-offset-2">
                Change chat provider
              </button>
            </div>
          )}
          {isLearningModel(ollamaRow, settings?.llm) ? (
            <LearningWindowNotice model={settings?.llm.learningModel ?? null} savedWindow={null} />
          ) : null}
        </div>

        <div className="flex shrink-0 flex-wrap items-center justify-end gap-1.5">
          {activeOllamaRoles.map((role) => (
            <ActiveRoleBadge key={role.id} roleId={role.id} label={role.label} />
          ))}

          <div className={cn('flex items-center gap-1.5', activeOllamaRoles.length > 0 && 'ml-1 border-l border-border-subtle pl-1.5')}>
            <IconButton
              label="Configure model"
              data-roles={ollamaRoles.map((role) => role.id).join(' ')}
              onClick={() => setShowConfigModal(true)}
            >
              <SlidersHorizontal />
            </IconButton>
            {/* Outside the lg-only meta line: on a narrow window the missing-tag
                state would otherwise be both invisible and unfixable. */}
            {!chatTag || !utilityTag ? (
              <button
                type="button"
                onClick={goToChatSettings}
                className="shrink-0 text-xs text-text-secondary transition-colors duration-fast hover:text-text-primary"
              >
                Set tags
              </button>
            ) : null}
          </div>
        </div>
      </div>
      {visionTags.length > 0 ? (
        <div className="mt-3 space-y-3 pl-12">
          {visionTags.map((tag) => (
            <ModelVisionDeclaration key={tag} modelRef={{ kind: 'ollama', model: tag }} modelName={tag} />
          ))}
        </div>
      ) : null}

      {showConfigModal ? (
        <ConnectionRolesDialog
          title="Ollama connection"
          servedLabel="Connected tags"
          served={meta}
          detail={ollamaUrl || undefined}
          hint="Choose which tasks this connection powers. Role changes take effect immediately."
          onClose={() => setShowConfigModal(false)}
          limits={visionTags.map((tag) => (
            <ModelWindowControls key={tag} modelRef={{ kind: 'ollama', model: tag }} modelName={tag} />
          ))}
        >
          {ollamaRoles.map((role) => {
            const tagForRole = role.id === 'utility' ? utilityTag : chatTag;
            return tagForRole ? (
              <RoleButton key={role.id} model={ollamaRow} role={role} />
            ) : (
              <RoleToggle
                key={role.id}
                role={role}
                active={false}
                disabled
                title={`Set the ${role.id === 'utility' ? 'utility' : 'chat'} tag in Chat settings first`}
                onToggle={() => undefined}
              />
            );
          })}
        </ConnectionRolesDialog>
      ) : null}
    </div>
  );
}
