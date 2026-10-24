import type { LLMSettings } from '@/types/api/settings';
import type { DownloadedModel } from '@/types/downloadedModels';
import { modelRefForRow, sameModelRef } from '@/utils/chatModelSelection';

export type RoleId = 'chat' | 'utility' | 'embedding' | 'learning' | 'verifier';

export interface RoleDescriptor {
  id: RoleId;
  label: string;
  assignLabel: string;
  pendingLabel: string;
  activeLabel: string;
  isActive: (model: DownloadedModel, settings: LLMSettings | undefined) => boolean;
  allowedTypes: ReadonlyArray<string>;
  hint: string;
}

export function isChatModelSelected(
  model: DownloadedModel,
  provider: LLMSettings['provider'] | undefined,
): boolean {
  return model.backend === 'ollama'
    ? provider === 'ollama'
    : model.is_active_for_chat && (provider === 'local' || provider === 'auto');
}

export function isUtilityModel(model: DownloadedModel, settings: LLMSettings | undefined): boolean {
  return model.is_active_for_utility && !settings?.utilityModel;
}

export function isLearningModel(model: DownloadedModel, settings: LLMSettings | undefined): boolean {
  return sameModelRef(settings?.learningModel, modelRefForRow(model, settings));
}

export function isVerifierModel(model: DownloadedModel, settings: LLMSettings | undefined): boolean {
  return sameModelRef(settings?.verifierModel, modelRefForRow(model, settings));
}

export function isReferenceRole(role: RoleId): role is 'learning' | 'verifier' {
  return role === 'learning' || role === 'verifier';
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
    isActive: isUtilityModel,
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
  {
    id: 'verifier',
    label: 'Verifier',
    assignLabel: 'Set as Verifier',
    pendingLabel: 'Setting…',
    activeLabel: 'Active (Verifier)',
    isActive: isVerifierModel,
    allowedTypes: ['language_model', 'chat', 'cloud'],
    hint: 'Audits every claim of a lesson against its cited passages before it publishes. May be a cloud model.',
  },
];

export function roleDescriptor(id: RoleId): RoleDescriptor {
  const descriptor = ROLES.find((role) => role.id === id);
  if (!descriptor) throw new Error(`No role descriptor for ${id}`);
  return descriptor;
}

export function canModelFulfillRole(
  model: Pick<DownloadedModel, 'model_type'>,
  role: RoleDescriptor,
): boolean {
  return role.allowedTypes.includes(model.model_type);
}
