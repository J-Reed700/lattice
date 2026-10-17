/**
 * Role configuration for model assignment.
 *
 * A single source of truth for the four roles a model can fulfill:
 * - `chat`      — primary LLM used for user-facing conversation
 * - `utility`   — fast small model for HyDE expansion, routing, intent
 * - `embedding` — semantic vector generator for search
 * - `learning`  — the model every learning job runs on; never a chat's
 *
 * Adding a role is a one-object append here: every consumer picks it up.
 * Chat, utility and embedding live as flags on the model rows; Learning lives
 * in Settings as a model reference, which can name an Ollama tag or a
 * llama.cpp model as well as a downloaded file.
 */

import type { LLMSettings } from '@/types/api/settings';
import type { DownloadedModel } from '@/types/downloadedModels';
import { modelRefForRow, sameModelRef } from '@/utils/chatModelSelection';

export type RoleId = 'chat' | 'utility' | 'embedding' | 'learning';

export interface RoleDescriptor {
  id: RoleId;
  label: string;
  /** Short verb used on buttons — "Set as Chat", "Set as Utility", etc. */
  assignLabel: string;
  /** Verb shown while the button is mid-flight. */
  pendingLabel: string;
  /** Verb shown when the role is already assigned. */
  activeLabel: string;
  /** Whether `model` holds this role now, given the saved settings. */
  isActive: (model: DownloadedModel, settings: LLMSettings | undefined) => boolean;
  /** Model types allowed for this role. Cards hide the button otherwise. */
  allowedTypes: ReadonlyArray<string>;
  /** Short helper text shown under the button / in the Ollama meta-card. */
  hint: string;
}

/** An explicit remote provider overrides any saved downloaded chat assignment. */
export function isChatModelSelected(
  model: DownloadedModel,
  provider: LLMSettings['provider'] | undefined,
): boolean {
  return model.backend === 'ollama'
    ? provider === 'ollama'
    : model.is_active_for_chat && (provider === 'local' || provider === 'auto');
}

/** The row names the model Settings saved as the Learning model. */
export function isLearningModel(model: DownloadedModel, settings: LLMSettings | undefined): boolean {
  return sameModelRef(settings?.learningModel, modelRefForRow(model, settings));
}

export const ROLES: ReadonlyArray<RoleDescriptor> = [
  {
    id: 'chat',
    label: 'Chat',
    assignLabel: 'Set as Chat',
    pendingLabel: 'Setting…',
    activeLabel: 'Active (Chat)',
    isActive: (model, settings) => isChatModelSelected(model, settings?.provider),
    allowedTypes: ['language_model', 'chat'],
    hint: 'The main model that answers the user.',
  },
  {
    id: 'utility',
    label: 'Utility',
    assignLabel: 'Set as Utility',
    pendingLabel: 'Setting…',
    activeLabel: 'Active (Utility)',
    isActive: (model) => model.is_active_for_utility,
    allowedTypes: ['language_model', 'chat'],
    hint: 'Fast small model for query expansion and routing (HyDE).',
  },
  {
    id: 'embedding',
    label: 'Embedding',
    assignLabel: 'Set as Embedding',
    pendingLabel: 'Setting…',
    activeLabel: 'Active (Embedding)',
    isActive: (model) => model.is_active_for_embedding,
    allowedTypes: ['text_embeddings', 'embedding'],
    hint: 'Generates vectors for semantic search.',
  },
  {
    id: 'learning',
    label: 'Learning',
    assignLabel: 'Set as Learning',
    pendingLabel: 'Setting…',
    activeLabel: 'Active (Learning)',
    isActive: isLearningModel,
    allowedTypes: ['language_model', 'chat'],
    hint: 'Prepares lessons, outlines, practice and study decks. Needs a large context window.',
  },
];

export function canModelFulfillRole(
  model: Pick<DownloadedModel, 'model_type'>,
  role: RoleDescriptor,
): boolean {
  return role.allowedTypes.includes(model.model_type);
}
