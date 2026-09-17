/**
 * OllamaMetaRow
 *
 * One row representing the Ollama server endpoint, backed by a single
 * synthetic row in the `models` table (`model_id = '__ollama_server__'`,
 * `backend = 'ollama'`). Toggling the role buttons here flips the same
 * `is_active_for_*` flags used by local models, so the SQLite single-active
 * triggers give us mutual exclusion for free.
 *
 * Model tags (chat / utility) live in LLM settings, not on the row — many
 * hosted Ollama deployments block `/api/tags` enumeration, so users type the
 * tags manually on the Chat settings page. This row shows them read-only.
 */

import { useModelRoles } from './ModelRolesContext';
import { RoleButton } from './RoleButton';
import { ROLES } from './roleConfig';
import { useSettingsQuery } from '../../../hooks/queries/useSettingsQuery';

const OLLAMA_SERVER_MODEL_ID = '__ollama_server__';

export function OllamaMetaRow() {
  const { models, isLoading } = useModelRoles();
  // The connection and tags live in LLM settings, and the settings repository
  // is the source of truth for them. Read-only here; Chat settings is where
  // they are edited.
  const { data: settings } = useSettingsQuery();

  const ollamaRow = models.find((m) => m.model_id === OLLAMA_SERVER_MODEL_ID);

  if (isLoading || !ollamaRow) {
    return null;
  }

  const ollamaUrl = settings?.llm.ollamaUrl.trim() ?? '';
  const chatTag = settings?.llm.model.trim() ?? '';
  const provider = settings?.llm.provider;
  // Chat and utility both resolve through the configured provider (see
  // ModelLoader::load and load_utility_remote), so either assignment is
  // overridden whenever the provider is not Ollama.
  const providerOverride =
    (ollamaRow.is_active_for_chat || ollamaRow.is_active_for_utility) &&
    provider &&
    provider !== 'ollama';
  const utilityTag = settings?.llm.ollamaUtilityModel.trim() ?? '';

  // Utility falls back to the chat tag when the user hasn't set a
  // separate one (mirrors the Rust side in get_or_load_utility_llm).
  const resolvedUtilityTag = utilityTag || chatTag;

  const ollamaRoles = ROLES.filter((r) => r.id === 'chat' || r.id === 'utility');

  const goToChatSettings = () => {
    window.dispatchEvent(new CustomEvent('settings:navigate-tab', { detail: { tab: 'chat' } }));
  };

  const meta = [
    'Ollama',
    ollamaUrl || 'no server URL',
    chatTag ? `assigned chat model ${chatTag}` : 'chat tag not set',
    resolvedUtilityTag ? `utility ${resolvedUtilityTag}` : 'utility tag not set',
  ].join(' · ');

  return (
    <div className="group flex items-center gap-4 border-b border-border-subtle py-3">
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
        <p className="mt-1 break-words text-xs text-text-muted">{meta}</p>
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
      </div>

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

      <div className="flex shrink-0 items-center gap-0.5">
        {ollamaRoles.map((role) => {
          const tagForRole = role.id === 'utility' ? resolvedUtilityTag : chatTag;
          if (!tagForRole) {
            return (
              <button
                key={role.id}
                type="button"
                disabled
                className="h-7 cursor-not-allowed rounded-sm px-2 text-xs text-text-disabled"
                title={`Set the ${role.label.toLowerCase()} tag in Chat settings first`}
              >
                {role.label}
              </button>
            );
          }
          return <RoleButton key={role.id} model={ollamaRow} role={role} />;
        })}
      </div>
    </div>
  );
}
