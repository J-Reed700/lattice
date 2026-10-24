import { useState } from 'react';

import { Network, SlidersHorizontal } from 'lucide-react';

import { IconButton } from '@/components/ui';
import { useSettingsQuery } from '@/features/settings/hooks/useSettingsQuery';
import { cn } from '@/lib/utils';

import { ConnectionRolesDialog } from './ConnectionRolesDialog';
import { LearningWindowNotice } from './LearningWindowNotice';
import { useModelRoles } from './ModelRolesContext';
import { ActiveRoleBadge, RoleButton, RoleToggle } from './RoleButton';
import { isLearningModel, isUtilityModel, ROLES } from './roleConfig';
import { ModelVisionDeclaration } from '../AITab/VisionDeclarationEditor';

const OLLAMA_SERVER_MODEL_ID = '__ollama_server__';

export function OllamaMetaRow() {
  const { models, isLoading } = useModelRoles();
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
  const provider = settings?.llm.provider;
  const utilityRowActive = isUtilityModel(ollamaRow, settings?.llm);
  // Chat and utility both resolve through the configured provider (see
  // ModelLoader::load and load_utility_remote), so either assignment is
  // overridden whenever the provider is not Ollama.
  const providerOverride =
    (ollamaRow.is_active_for_chat || utilityRowActive) &&
    provider &&
    provider !== 'ollama';
  const utilityTag = settings?.llm.ollamaUtilityModel.trim() ?? '';

  // Utility falls back to the chat tag when the user hasn't set a
  // separate one (mirrors the Rust side in get_or_load_utility_llm).
  const resolvedUtilityTag = utilityTag || chatTag;

  // Learning names the chat tag on this connection: `Ollama{tag}` in Settings.
  const ollamaRoles = ROLES.filter((r) => r.id === 'chat' || r.id === 'utility' || r.id === 'learning' || r.id === 'verifier');
  const activeOllamaRoles = ollamaRoles.filter((role) => role.isActive(ollamaRow, settings?.llm));

  const goToChatSettings = () => {
    window.dispatchEvent(new CustomEvent('settings:navigate-tab', { detail: { tab: 'chat' } }));
  };

  // Each Ollama model that can answer a chat carries its own vision
  // declaration: the chat tag, then any tag downloaded as a row of its own.
  const visionTags = Array.from(new Set([
    chatTag,
    ...models
      .filter((m) => m.backend === 'ollama' && m.model_id !== OLLAMA_SERVER_MODEL_ID
        && (m.model_type === 'language_model' || m.model_type === 'chat'))
      .map((m) => m.model_id),
  ].filter(Boolean)));

  const meta = [
    chatTag ? `Chat · ${chatTag}` : 'Chat tag not set',
    resolvedUtilityTag ? `Utility · ${resolvedUtilityTag}` : 'Utility tag not set',
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
          {providerOverride && (
            <div role="status" className="mt-2 max-w-xl text-xs text-text-secondary">
              {provider === 'auto'
                ? 'Chat provider is Auto: for both chat and utility, local models and llama.cpp take priority over this Ollama assignment.'
                : `Chat provider is ${provider === 'llamacpp' ? 'llama.cpp' : provider}: chat and utility use that provider, not this Ollama assignment.`}{' '}
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
            {!chatTag || !resolvedUtilityTag ? (
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
        >
          {ollamaRoles.map((role) => {
            const tagForRole = role.id === 'utility' ? resolvedUtilityTag : chatTag;
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
