/**
 * Role configuration for model assignment.
 *
 * A single source of truth for the three roles a model can fulfill:
 * - `chat`      — primary LLM used for user-facing conversation
 * - `utility`   — fast small model for HyDE expansion, routing, intent
 * - `embedding` — semantic vector generator for search
 *
 * Adding a fourth role (e.g. "reranker") is a one-object append here —
 * every consumer picks up the new role automatically.
 */

import type { DownloadedModel } from '../../../types/downloadedModels';

export type RoleId = 'chat' | 'utility' | 'embedding';

export interface RoleDescriptor {
  id: RoleId;
  label: string;
  /** Short verb used on buttons — "Set as Chat", "Set as Utility", etc. */
  assignLabel: string;
  /** Verb shown while the button is mid-flight. */
  pendingLabel: string;
  /** Verb shown when the role is already assigned. */
  activeLabel: string;
  /** Field on `DownloadedModel` that indicates this role is currently active. */
  activeKey: keyof DownloadedModel;
  /** Model types allowed for this role. Cards hide the button otherwise. */
  allowedTypes: ReadonlyArray<string>;
  /** Short helper text shown under the button / in the Ollama meta-card. */
  hint: string;
}

export const ROLES: ReadonlyArray<RoleDescriptor> = [
  {
    id: 'chat',
    label: 'Chat',
    assignLabel: 'Set as Chat',
    pendingLabel: 'Setting…',
    activeLabel: 'Active (Chat)',
    activeKey: 'is_active_for_chat',
    allowedTypes: ['language_model', 'chat'],
    hint: 'The main model that answers the user.',
  },
  {
    id: 'utility',
    label: 'Utility',
    assignLabel: 'Set as Utility',
    pendingLabel: 'Setting…',
    activeLabel: 'Active (Utility)',
    activeKey: 'is_active_for_utility',
    allowedTypes: ['language_model', 'chat'],
    hint: 'Fast small model for query expansion and routing (HyDE).',
  },
  {
    id: 'embedding',
    label: 'Embedding',
    assignLabel: 'Set as Embedding',
    pendingLabel: 'Setting…',
    activeLabel: 'Active (Embedding)',
    activeKey: 'is_active_for_embedding',
    allowedTypes: ['text_embeddings', 'embedding'],
    hint: 'Generates vectors for semantic search.',
  },
];

export function canModelFulfillRole(
  model: Pick<DownloadedModel, 'model_type'>,
  role: RoleDescriptor,
): boolean {
  return role.allowedTypes.includes(model.model_type);
}

export function isModelActiveForRole(
  model: DownloadedModel,
  role: RoleDescriptor,
): boolean {
  return Boolean(model[role.activeKey]);
}
