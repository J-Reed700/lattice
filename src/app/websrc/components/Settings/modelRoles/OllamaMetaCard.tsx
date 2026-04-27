/**
 * OllamaMetaCard
 *
 * One card in the AI Models grid representing the Ollama server endpoint.
 * It is backed by a single synthetic row in the `models` table
 * (`model_id = '__ollama_server__'`, `backend = 'ollama'`). Toggling the
 * role buttons here flips the same `is_active_for_*` flags used by local
 * model cards, so the SQLite single-active triggers give us mutual
 * exclusion for free.
 *
 * Model tags (chat / utility) live in LLM settings, not on the row — many
 * hosted Ollama deployments block `/api/tags` enumeration, so users type
 * the tags manually on the Chat settings page. This card shows them
 * read-only with a pointer.
 */

import { useEffect, useState } from 'react';

import { Cloud } from 'lucide-react';

import Card from '../../ui/Card/Card';
import { Icon } from '../../ui/Icon';
import { VaultAPI } from '../../../lib/api';
import { RoleButton } from './RoleButton';
import { ROLES } from './roleConfig';
import { useModelRoles } from './ModelRolesContext';

const OLLAMA_SERVER_MODEL_ID = '__ollama_server__';

interface OllamaConfig {
  url: string;
  chatTag: string;
  utilityTag: string;
}

export function OllamaMetaCard() {
  const { models, isLoading } = useModelRoles();
  const [config, setConfig] = useState<OllamaConfig>({ url: '', chatTag: '', utilityTag: '' });

  // Pull the Ollama connection + tag config straight from the backend. It
  // lives on the Chat settings page; we just display it read-only here.
  useEffect(() => {
    let alive = true;
    (async () => {
      const result = await VaultAPI.getSettings();
      if (!alive || !result.ok) return;
      const llm = result.data.llm as {
        ollamaUrl?: string;
        model?: string;
        ollamaUtilityModel?: string;
      };
      setConfig({
        url: (llm.ollamaUrl ?? '').trim(),
        chatTag: (llm.model ?? '').trim(),
        utilityTag: (llm.ollamaUtilityModel ?? '').trim(),
      });
    })();
    return () => {
      alive = false;
    };
  }, [models]); // reload config when the role list changes

  // Find the synthetic server row. Hide the card entirely if it isn't
  // there yet (e.g. mid-migration on first launch) — no dead UI.
  const ollamaRow = models.find((m) => m.model_id === OLLAMA_SERVER_MODEL_ID);

  if (isLoading || !ollamaRow) {
    return null;
  }

  const { url: ollamaUrl, chatTag, utilityTag } = config;

  // Utility role falls through to the chat tag when no utility tag is set.
  const effectiveUtilityTag = utilityTag || chatTag;

  // Filter the roles array for this card: we only surface chat + utility on
  // Ollama (embedding uses a separate local Candle pipeline).
  const ollamaRoles = ROLES.filter((r) => r.id === 'chat' || r.id === 'utility');

  return (
    <Card padding="md" className="h-full">
      <div className="flex flex-col h-full">
        <div className="flex items-start justify-between gap-2 mb-3">
          <div className="flex-1 min-w-0">
            <span className="inline-flex items-center px-1.5 py-0.5 text-[10px] font-medium rounded bg-[hsl(var(--accent-muted))] text-[hsl(var(--accent))] border border-[hsl(var(--accent-muted))] uppercase tracking-wide">
              Ollama
            </span>
            <h3 className="font-semibold text-sm text-[hsl(var(--text-primary))] mt-1 flex items-center gap-2">
              <Icon as={Cloud} size={16} /> Ollama Server
            </h3>
            <p className="text-xs text-[hsl(var(--text-tertiary))] mt-0.5 truncate">
              {ollamaUrl || 'No server URL — set one in Chat settings'}
            </p>
          </div>
        </div>

        <div className="flex-1 space-y-2 mb-3 text-xs">
          <TagRow label="Chat tag" value={chatTag} fallback="Not set" />
          <TagRow
            label="Utility tag"
            value={utilityTag}
            fallback={chatTag ? `Falls back to chat (${chatTag})` : 'Not set'}
            muted={!utilityTag}
          />
          <p className="text-[10px] text-[hsl(var(--text-tertiary))] leading-snug pt-1">
            Edit these tags in Settings → Chat → Ollama Server Connection.
          </p>
        </div>

        <div className="flex flex-col gap-2 pt-3 border-t border-[hsl(var(--border-subtle))]">
          <div className="flex flex-wrap gap-1.5">
            {ollamaRoles.map((role) => {
              // The utility role button is disabled when there's no tag at
              // all — activating it would immediately fall back to chat,
              // which is confusing.
              const tagForRole = role.id === 'chat' ? chatTag : effectiveUtilityTag;
              if (!tagForRole) {
                return (
                  <button
                    key={role.id}
                    type="button"
                    disabled
                    className="flex-1 min-w-0 px-2.5 py-1 text-xs rounded border border-[hsl(var(--border-subtle))] text-[hsl(var(--text-tertiary))] cursor-not-allowed"
                    title={`Set the ${role.label.toLowerCase()} tag in Chat settings first`}
                  >
                    {role.assignLabel}
                  </button>
                );
              }
              return <RoleButton key={role.id} model={ollamaRow} role={role} />;
            })}
          </div>
        </div>
      </div>
    </Card>
  );
}

interface TagRowProps {
  label: string;
  value: string;
  fallback: string;
  muted?: boolean;
}

function TagRow({ label, value, fallback, muted }: TagRowProps) {
  return (
    <div className="flex items-center justify-between gap-2">
      <span className="text-[hsl(var(--text-secondary))]">{label}</span>
      <code
        className={`truncate text-[11px] ${
          muted
            ? 'text-[hsl(var(--text-tertiary))]'
            : 'text-[hsl(var(--text-primary))]'
        }`}
      >
        {value || fallback}
      </code>
    </div>
  );
}
