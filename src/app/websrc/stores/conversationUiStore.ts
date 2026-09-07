import { create } from 'zustand';

import type { ConversationFilterMode } from './conversationsStore.types';
import type { OptimisticMessage, RetrievalTrace } from '../types/conversation';

export interface ConversationUiState {
  selectedSpaceId: string | null;
  filterMode: ConversationFilterMode;
  searchQuery: string;
  activeConversationId: string | null;
  inFlightGenerations: Map<string, string>;
  optimisticMessages: Map<string, OptimisticMessage>;
  /**
   * The in-flight retrieval trace per conversation. This is the live view of an
   * event stream, not backend state — it is replaced by the persisted trace in
   * the message metadata the moment the turn finishes.
   */
  liveRetrieval: Map<string, RetrievalTrace>;
  /** Text the composer should adopt on its next render (deep links, retries). */
  composerDraft: string | null;
  error: string | null;
  requestedLinkedConversationIds: Set<string>;
  requestedWebSourceConversationIds: Set<string>;
  requestedMembershipDocumentIds: Set<string>;
}

export const useConversationUiStore = create<ConversationUiState>(() => ({
  selectedSpaceId: null,
  filterMode: 'all',
  searchQuery: '',
  activeConversationId: null,
  inFlightGenerations: new Map(),
  optimisticMessages: new Map(),
  liveRetrieval: new Map(),
  composerDraft: null,
  error: null,
  requestedLinkedConversationIds: new Set(),
  requestedWebSourceConversationIds: new Set(),
  requestedMembershipDocumentIds: new Set(),
}));

export const conversationUiStore = useConversationUiStore;
