import { listen } from '@tauri-apps/api/event';
import { create } from 'zustand';

import { VaultAPI } from '@/lib/api';

import { useDownloadedModelsStore } from './downloadedModelsStore';

import type {
  OptimisticMessage,
  DisplayMessage,
  Conversation,
  SourceWithMetadata,
  MessageVerificationSummary,
  ConversationMessage,
  ConversationMessageBookmark,
  ToolPreferences,
} from '../types/conversation';
import { ErrorCode } from '../types/api/errorCodes';
import type {
  ConversationSpaceDto,
  ConversationLinkedDocumentDto,
  DocumentSpaceMembershipDto,
} from '../types';
import {
  MessageVerificationSummarySchema,
  SourceWithMetadataSchema,
  SourcesArraySchema,
} from '../types/conversation';

const normalizeChunkExcerpt = (raw: unknown): Record<string, unknown> | null => {
  if (!raw || typeof raw !== 'object') return null;
  const value = raw as Record<string, unknown>;
  const chunkId = String(value.chunkId ?? value.chunk_id ?? '').trim();
  const excerpt = String(value.excerpt ?? '').trim();
  if (!chunkId || !excerpt) return null;
  const scoreRaw = Number(value.score ?? 0);
  const score = Number.isFinite(scoreRaw) && scoreRaw >= 0 ? scoreRaw : 0;
  return {
    chunkId,
    excerpt,
    section: value.section ?? undefined,
    chunkIndex: value.chunkIndex ?? value.chunk_index ?? undefined,
    score,
    highlights: value.highlights ?? undefined,
  };
};

const normalizeSource = (raw: unknown): Record<string, unknown> | null => {
  if (!raw || typeof raw !== 'object') return null;
  const value = raw as Record<string, unknown>;
  const filePath = String(value.filePath ?? value.file_path ?? '').trim();
  const fileName = String(value.fileName ?? value.file_name ?? '').trim();
  const content = String(value.content ?? '').trim();
  const documentId = String(value.documentId ?? value.document_id ?? '').trim();
  const chunkId = String(value.chunkId ?? value.chunk_id ?? '').trim();
  const resolvedPath = filePath || 'unknown://source';
  const resolvedName = fileName || resolvedPath;
  const resolvedDocumentId = documentId || `source:${resolvedPath}`;
  const resolvedChunkId = chunkId || `${resolvedDocumentId}#chunk`;
  const scoreRaw = Number(value.score ?? 0);
  const score = Number.isFinite(scoreRaw) && scoreRaw >= 0 ? scoreRaw : 0;
  const chunkExcerptsRaw = Array.isArray(value.chunkExcerpts)
    ? value.chunkExcerpts
    : (Array.isArray(value.chunk_excerpts) ? value.chunk_excerpts : []);

  return {
    documentId: resolvedDocumentId,
    chunkId: resolvedChunkId,
    fileName: resolvedName,
    filePath: resolvedPath,
    mimeType: value.mimeType ?? value.mime_type ?? 'text/plain',
    category: value.category ?? (resolvedPath.startsWith('http') ? 'Web Article' : 'Unknown'),
    content,
    excerpt: value.excerpt ?? undefined,
    highlights: value.highlights ?? undefined,
    section: value.section ?? undefined,
    chunkIndex: value.chunkIndex ?? value.chunk_index ?? undefined,
    chunkExcerpts: chunkExcerptsRaw
      .map(normalizeChunkExcerpt)
      .filter((chunk): chunk is Record<string, unknown> => chunk !== null),
    score,
    fileSizeBytes: value.fileSizeBytes ?? value.file_size_bytes ?? 0,
    modifiedAt: value.modifiedAt ?? value.modified_at ?? '',
  };
};

const parsePersistedSources = (rawSources: unknown): SourceWithMetadata[] => {
  if (!Array.isArray(rawSources)) return [];

  const normalized = rawSources
    .map(normalizeSource)
    .filter((source): source is Record<string, unknown> => source !== null);
  if (normalized.length === 0) return [];

  const strictArray = SourcesArraySchema.safeParse(normalized);
  if (strictArray.success) return strictArray.data;

  const recovered: SourceWithMetadata[] = [];
  for (const source of normalized) {
    const parsed = SourceWithMetadataSchema.safeParse(source);
    if (parsed.success) {
      recovered.push(parsed.data);
    }
  }
  return recovered;
};

const parsePersistedVerification = (rawVerification: unknown): MessageVerificationSummary | null => {
  if (!rawVerification || typeof rawVerification !== 'object') return null;
  const value = rawVerification as Record<string, unknown>;
  const normalized = {
    enabled: Boolean(value.enabled),
    claimsEvaluated: value.claimsEvaluated ?? value.claims_evaluated,
    supportedClaims: value.supportedClaims ?? value.supported_claims,
    supportedClaimNotes: value.supportedClaimNotes ?? value.supported_claim_notes,
    unsupportedClaims: value.unsupportedClaims ?? value.unsupported_claims,
    groundedRatio: value.groundedRatio ?? value.grounded_ratio,
  };
  const parsed = MessageVerificationSummarySchema.safeParse(normalized);
  return parsed.success ? parsed.data : null;
};

const pendingCancellationConversations = new Set<string>();

const isUserInitiatedCancellation = (
  conversationId: string,
  errorCode?: string
): boolean => {
  if (!pendingCancellationConversations.has(conversationId)) {
    return false;
  }

  return (
    errorCode === ErrorCode.INVALID_STATE ||
    errorCode === ErrorCode.INTERNAL_ERROR
  );
};


export interface Message {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  status: 'pending' | 'completed' | 'failed';
  createdAt: string;
}

export type ConversationFilterMode =
  | 'all'
  | 'saved'
  | 'bookmarked'
  | 'pinned'
  | 'archived'
  | 'snippets';

interface LoadConversationsOverrides {
  spaceId?: string | null;
  filterMode?: ConversationFilterMode;
  searchQuery?: string;
}

interface LoadMessageBookmarksOverrides {
  conversationId?: string;
  query?: string;
}

interface ConversationsState {
  spaces: ConversationSpaceDto[];
  selectedSpaceId: string | null;
  filterMode: ConversationFilterMode;
  searchQuery: string;
  conversations: Conversation[];
  messageBookmarks: ConversationMessageBookmark[];
  messageBookmarkMap: Map<string, ConversationMessageBookmark>;
  activeConversationId: string | null;
  isLoading: boolean;
  isLoadingBookmarks: boolean;
  isSending: boolean;
  error: string | null;
  optimisticMessages: Map<string, OptimisticMessage>;
  /** Sources from the last assistant message (for citation display) */
  lastMessageSources: Map<string, SourceWithMetadata[]>;
  /** Verification summary by assistant message ID */
  messageVerification: Map<string, MessageVerificationSummary>;
  /** Linked documents per conversation */
  linkedDocumentsByConversationId: Map<string, ConversationLinkedDocumentDto[]>;
  /** Space assignments per document */
  documentSpaceMembershipsByDocumentId: Map<string, DocumentSpaceMembershipDto[]>;

  loadSpaces: () => Promise<void>;
  setSelectedSpace: (spaceId: string | null) => void;
  setFilterMode: (mode: ConversationFilterMode) => void;
  setSearchQuery: (query: string) => void;
  loadConversations: (overrides?: LoadConversationsOverrides) => Promise<void>;
  loadMessageBookmarks: (overrides?: LoadMessageBookmarksOverrides) => Promise<void>;
  createConversation: (title: string) => Promise<string>;
  setConversationSaved: (id: string, value: boolean) => Promise<void>;
  setConversationBookmarked: (id: string, value: boolean) => Promise<void>;
  setConversationPinned: (id: string, value: boolean) => Promise<void>;
  setConversationArchived: (id: string, value: boolean) => Promise<void>;
  bookmarkMessage: (
    conversationId: string,
    messageId: string,
    title?: string | null,
    note?: string | null
  ) => Promise<void>;
  unbookmarkMessage: (conversationId: string, messageId: string) => Promise<void>;
  moveConversationToSpace: (id: string, spaceId: string) => Promise<void>;
  loadConversationLinkedDocuments: (conversationId: string) => Promise<void>;
  removeConversationLinkedDocument: (conversationId: string, documentId: string) => Promise<void>;
  loadDocumentSpaceMemberships: (documentId: string) => Promise<void>;
  setDocumentSpaceMembership: (
    documentId: string,
    spaceId: string,
    assigned: boolean
  ) => Promise<void>;
  selectConversation: (id: string) => Promise<void>;
  sendMessage: (
    content: string,
    conversationId?: string | null,
    toolPreferences?: ToolPreferences
  ) => Promise<void>;
  cancelGeneration: (conversationId?: string | null) => Promise<void>;
  deleteConversation: (id: string) => Promise<void>;
  clearError: () => void;
}

export const useConversationsStore = create<ConversationsState>((set, get) => ({
  spaces: [],
  selectedSpaceId: null,
  filterMode: 'all',
  searchQuery: '',
  conversations: [],
  messageBookmarks: [],
  messageBookmarkMap: new Map(),
  activeConversationId: null,
  isLoading: false,
  isLoadingBookmarks: false,
  isSending: false,
  error: null,
  optimisticMessages: new Map(),
  lastMessageSources: new Map(),
  messageVerification: new Map(),
  linkedDocumentsByConversationId: new Map(),
  documentSpaceMembershipsByDocumentId: new Map(),

  loadSpaces: async () => {
    const result = await VaultAPI.listConversationSpaces();
    if (!result.ok) {
      set({ error: result.error });
      return;
    }
    set({ spaces: result.data });
  },

  setSelectedSpace: (spaceId: string | null) => set({ selectedSpaceId: spaceId }),
  setFilterMode: (mode: ConversationFilterMode) => set({ filterMode: mode }),
  setSearchQuery: (query: string) => set({ searchQuery: query }),

  loadConversations: async (overrides?: LoadConversationsOverrides) => {
    if (overrides) {
      set((state) => ({
        selectedSpaceId: overrides.spaceId ?? state.selectedSpaceId,
        filterMode: overrides.filterMode ?? state.filterMode,
        searchQuery: overrides.searchQuery ?? state.searchQuery,
      }));
    }

    const selectedSpaceId = overrides?.spaceId ?? get().selectedSpaceId;
    const filterMode = overrides?.filterMode ?? get().filterMode;
    const searchQuery = overrides?.searchQuery ?? get().searchQuery;
    const trimmedQuery = searchQuery.trim();
    const requiresExplorerQuery = Boolean(
      selectedSpaceId ||
      trimmedQuery ||
      filterMode === 'saved' ||
      filterMode === 'bookmarked' ||
      filterMode === 'pinned' ||
      filterMode === 'snippets' ||
      filterMode === 'archived'
    );

    set({ isLoading: true, error: null });
    const explorerResult = await VaultAPI.listConversationsExplorer({
      spaceId: selectedSpaceId ?? undefined,
      query: trimmedQuery ? trimmedQuery : undefined,
      savedOnly: filterMode === 'saved' ? true : undefined,
      bookmarkedOnly: filterMode === 'bookmarked' ? true : undefined,
      pinnedOnly: filterMode === 'pinned' ? true : undefined,
      hasMessageBookmarks: filterMode === 'snippets' ? true : undefined,
      includeArchived: filterMode === 'archived',
    });

    if (!explorerResult.ok && requiresExplorerQuery) {
      set({
        error: explorerResult.error,
        isLoading: false,
      });
      return;
    }

    const result = explorerResult.ok ? explorerResult : await VaultAPI.listConversations();

    if (result.ok) {
      const conversationList = Array.isArray(result.data)
        ? result.data
        : result.data.conversations;

      const filteredConversationList = filterMode === 'archived'
        ? conversationList.filter((conv) => Boolean(conv.isArchived))
        : conversationList;

      set((state) => {
        // Preserve already-loaded message history to avoid UI "disappearing chat"
        // when list refreshes (e.g., initial mount races with first send).
        const existingById = new Map(state.conversations.map((conv) => [conv.id, conv]));
        const conversations = filteredConversationList.map((conv) => {
          const existing = existingById.get(conv.id);
          return {
            ...conv,
            messages: existing?.messages ?? [],
          };
        });

        const activeConversationStillExists = state.activeConversationId
          ? conversations.some((conv) => conv.id === state.activeConversationId)
          : true;
        const nextActiveConversationId = activeConversationStillExists
          ? state.activeConversationId
          : (conversations[0]?.id ?? null);

        return {
          conversations,
          isLoading: false,
          activeConversationId: nextActiveConversationId,
        };
      });
    } else {
      set({ error: result.error, isLoading: false });
    }
  },

  loadMessageBookmarks: async (overrides?: LoadMessageBookmarksOverrides) => {
    const conversationId = overrides?.conversationId ?? get().activeConversationId ?? undefined;
    if (!conversationId) {
      set({ messageBookmarks: [], messageBookmarkMap: new Map() });
      return;
    }

    set({ isLoadingBookmarks: true });
    const result = await VaultAPI.listMessageBookmarks({
      conversationId,
      query: overrides?.query?.trim() ? overrides.query.trim() : undefined,
      limit: 200,
      offset: 0,
    });

    if (!result.ok) {
      set({ error: result.error, isLoadingBookmarks: false });
      return;
    }

    const bookmarks = Array.isArray(result.data)
      ? []
      : result.data.bookmarks;
    const messageBookmarkMap = new Map(
      bookmarks.map((bookmark) => [bookmark.messageId, bookmark] as const)
    );

    set({
      messageBookmarks: bookmarks,
      messageBookmarkMap,
      isLoadingBookmarks: false,
    });
  },

  createConversation: async (title: string) => {
    const selectedSpaceId = get().selectedSpaceId;
    const selectedSpace = selectedSpaceId
      ? get().spaces.find((space) => space.id === selectedSpaceId)
      : undefined;

    // Get active chat model from downloadedModelsStore
    const activeModel = useDownloadedModelsStore.getState().activeModel;

    const settingsResult = await VaultAPI.getSettings();
    const llmSettings = settingsResult.ok ? settingsResult.data.llm : null;
    const provider = llmSettings?.provider ?? 'auto';
    const hasOllamaConfig = Boolean(llmSettings?.ollamaUrl && llmSettings?.model);

    let modelName: string | null = null;

    const spaceDefaultModel = selectedSpace?.defaultModelName?.trim() ?? '';
    if (spaceDefaultModel) {
      modelName = spaceDefaultModel;
    } else if (provider === 'ollama') {
      if (!hasOllamaConfig || !llmSettings) {
        const errorMsg =
          'No Ollama configuration found. Configure Ollama in Settings or select a local model.';
        set({ error: errorMsg });
        throw new Error(errorMsg);
      }
      modelName = llmSettings.model;
    } else if (provider === 'auto') {
      if (activeModel) {
        modelName = activeModel.model_id;
      } else if (hasOllamaConfig && llmSettings) {
        modelName = llmSettings.model;
      }
    } else {
      if (activeModel) {
        modelName = activeModel.model_id;
      }
    }

    if (!modelName) {
      const errorMsg =
        'No chat model available. Select a local model in Settings or configure Ollama.';
      set({ error: errorMsg });
      throw new Error(errorMsg);
    }

    // Call backend with required parameters
    const result = await VaultAPI.createConversation(title, modelName);
    if (!result.ok) {
      set({ error: result.error });
      throw new Error(result.error);
    }

    const id = result.data.conversation.id;
    if (selectedSpaceId && selectedSpaceId !== 'space_general') {
      const moveResult = await VaultAPI.moveConversationToSpace({
        conversationId: id,
        spaceId: selectedSpaceId,
      });
      if (!moveResult.ok) {
        set({ error: moveResult.error });
      }
    }
    await get().loadConversations();
    set({ activeConversationId: id });
    return id;
  },

  setConversationSaved: async (id: string, value: boolean) => {
    const result = await VaultAPI.setConversationSaved({ conversationId: id, value });
    if (!result.ok) {
      set({ error: result.error });
      return;
    }
    await get().loadConversations();
  },

  setConversationBookmarked: async (id: string, value: boolean) => {
    const result = await VaultAPI.setConversationBookmarked({ conversationId: id, value });
    if (!result.ok) {
      set({ error: result.error });
      return;
    }
    await get().loadConversations();
  },

  setConversationPinned: async (id: string, value: boolean) => {
    const result = await VaultAPI.setConversationPinned({ conversationId: id, value });
    if (!result.ok) {
      set({ error: result.error });
      return;
    }
    await get().loadConversations();
  },

  setConversationArchived: async (id: string, value: boolean) => {
    const result = await VaultAPI.setConversationArchived({ conversationId: id, value });
    if (!result.ok) {
      set({ error: result.error });
      return;
    }
    await get().loadConversations();
    if (value && get().activeConversationId === id) {
      set({ activeConversationId: null });
    }
  },

  bookmarkMessage: async (
    conversationId: string,
    messageId: string,
    title?: string | null,
    note?: string | null
  ) => {
    const result = await VaultAPI.bookmarkConversationMessage({
      conversationId,
      messageId,
      title,
      note,
    });

    if (!result.ok) {
      set({ error: result.error });
      return;
    }

    await get().loadMessageBookmarks({ conversationId });
  },

  unbookmarkMessage: async (conversationId: string, messageId: string) => {
    const result = await VaultAPI.unbookmarkConversationMessage({
      conversationId,
      messageId,
    });

    if (!result.ok) {
      set({ error: result.error });
      return;
    }

    await get().loadMessageBookmarks({ conversationId });
  },

  moveConversationToSpace: async (id: string, spaceId: string) => {
    const result = await VaultAPI.moveConversationToSpace({
      conversationId: id,
      spaceId,
    });
    if (!result.ok) {
      set({ error: result.error });
      return;
    }
    await get().loadConversations();
  },

  loadConversationLinkedDocuments: async (conversationId: string) => {
    const result = await VaultAPI.listConversationLinkedDocuments(conversationId);
    if (!result.ok) {
      set({ error: result.error });
      return;
    }

    set((state) => {
      const next = new Map(state.linkedDocumentsByConversationId);
      next.set(conversationId, result.data);
      return { linkedDocumentsByConversationId: next };
    });
  },

  removeConversationLinkedDocument: async (conversationId: string, documentId: string) => {
    const result = await VaultAPI.removeConversationLinkedDocument(conversationId, documentId);
    if (!result.ok) {
      set({ error: result.error });
      return;
    }

    set((state) => {
      const linked = new Map(state.linkedDocumentsByConversationId);
      const existing = linked.get(conversationId) ?? [];
      linked.set(
        conversationId,
        existing.filter((item) => item.documentId !== documentId)
      );

      const memberships = new Map(state.documentSpaceMembershipsByDocumentId);
      memberships.delete(documentId);

      return {
        linkedDocumentsByConversationId: linked,
        documentSpaceMembershipsByDocumentId: memberships,
      };
    });
  },

  loadDocumentSpaceMemberships: async (documentId: string) => {
    const result = await VaultAPI.listDocumentSpaceMemberships(documentId);
    if (!result.ok) {
      set({ error: result.error });
      return;
    }

    set((state) => {
      const next = new Map(state.documentSpaceMembershipsByDocumentId);
      next.set(documentId, result.data);
      return { documentSpaceMembershipsByDocumentId: next };
    });
  },

  setDocumentSpaceMembership: async (documentId: string, spaceId: string, assigned: boolean) => {
    const result = await VaultAPI.setDocumentSpaceMembership(documentId, spaceId, assigned);
    if (!result.ok) {
      set({ error: result.error });
      return;
    }

    await get().loadDocumentSpaceMemberships(documentId);
  },

  selectConversation: async (id: string) => {
    set({ isLoading: true, activeConversationId: id });
    const result = await VaultAPI.getConversationMessages(id);

    if (result.ok) {
      const loadedMessages: ConversationMessage[] = Array.isArray(result.data)
        ? result.data
        : result.data.messages;
      const existingConversation = get().conversations.find((conv) => conv.id === id);
      let conversations = get().conversations.map(conv =>
        conv.id === id ? { ...conv, messages: loadedMessages } : conv
      );

      if (!existingConversation) {
        const conversationResult = await VaultAPI.getConversation(id);
        if (conversationResult.ok) {
          const payload = Array.isArray(conversationResult.data)
            ? null
            : conversationResult.data.conversation;
          if (payload) {
            conversations = [
              {
                ...payload,
                messages: loadedMessages,
              },
              ...conversations,
            ];
          }
        }
      }

      // Restore sources + verification summaries from persisted message metadata.
      const restoredSources = new Map(get().lastMessageSources);
      const restoredVerification = new Map(get().messageVerification);
      for (const msg of loadedMessages) {
        if (msg.role === 'assistant' && msg.metadata) {
          try {
            const parsed = JSON.parse(msg.metadata);
            if (parsed) {
              const recoveredSources = parsePersistedSources(
                (parsed as Record<string, unknown>).sources
              );
              if (recoveredSources.length > 0) {
                restoredSources.set(msg.id, recoveredSources);
              }
              const verification = parsePersistedVerification(
                (parsed as Record<string, unknown>).verification
              );
              if (verification) {
                restoredVerification.set(msg.id, verification);
              }
            }
          } catch {
            // Ignore invalid metadata JSON
          }
        }
      }

      set({
        conversations,
        lastMessageSources: restoredSources,
        messageVerification: restoredVerification,
        isLoading: false,
      });
      await Promise.all([
        get().loadMessageBookmarks({ conversationId: id }),
        get().loadConversationLinkedDocuments(id),
      ]);
    } else {
      set({ error: result.error, isLoading: false });
    }
  },

  sendMessage: async (
    content: string,
    conversationId?: string | null,
    toolPreferences?: ToolPreferences
  ) => {
    // BUG 1 FIX: Add comprehensive logging to track follow-up message failures
    const { activeConversationId, optimisticMessages, conversations: existingConversations } = get();
    const requestConversationId =
      conversationId ?? activeConversationId ?? existingConversations[0]?.id ?? null;

    if (!requestConversationId) {
      set({
        error: 'No active conversation selected. Create or select a conversation first.',
      });
      return;
    }

    if (import.meta.env.DEV) {
      console.log('[ConversationsStore] sendMessage START', {
        activeConversationId: requestConversationId,
        contentLength: content.length,
        optimisticMessagesCount: optimisticMessages.size,
      });
    }

    // Generate unique temporary ID for optimistic message
    const tempId = crypto.randomUUID();

    // Create optimistic message BEFORE backend call
    const optimisticMsg: OptimisticMessage = {
      tempId,
      conversationId: requestConversationId,
      content,
      role: 'user',
      status: 'pending',
      createdAt: new Date().toISOString(),
    };

    // Add to optimistic messages Map (create new Map for reactivity)
    const newOptimisticMessages = new Map(optimisticMessages);
    newOptimisticMessages.set(tempId, optimisticMsg);

    set({
      optimisticMessages: newOptimisticMessages,
      isSending: true,
      error: null,
    });

    if (import.meta.env.DEV) {
      console.log('[ConversationsStore] Calling chat_with_conversation', {
        conversationId: requestConversationId,
        messageLength: content.length,
      });
    }

    // Create optimistic assistant message for streaming
    const assistantTempId = crypto.randomUUID();
    const assistantOptimisticMsg: OptimisticMessage = {
      tempId: assistantTempId,
      conversationId: requestConversationId,
      content: '',
      role: 'assistant',
      status: 'pending',
      createdAt: new Date().toISOString(),
    };

    // Add assistant message to optimistic messages
    const messagesWithAssistant = new Map(get().optimisticMessages);
    messagesWithAssistant.set(assistantTempId, assistantOptimisticMsg);
    set({ optimisticMessages: messagesWithAssistant });

    // Setup streaming listener
    const RETRYING_NOTICE = 'Model returned an empty response. Retrying...';
    const unlistenFn = await listen<{ content?: string; done?: boolean; status?: string; attempt?: number }>('llm-stream', (event) => {
      const { content: chunk, done, status, attempt } = event.payload;

      if (status === 'retrying') {
        const retryMessage = typeof attempt === 'number'
          ? `${RETRYING_NOTICE} (attempt ${attempt})`
          : RETRYING_NOTICE;
        set(state => {
          const updatedOptimistic = new Map(state.optimisticMessages);
          const currentMsg = updatedOptimistic.get(assistantTempId);
          if (currentMsg) {
            updatedOptimistic.set(assistantTempId, {
              ...currentMsg,
              content: retryMessage,
            });
          }
          return { optimisticMessages: updatedOptimistic };
        });
        return;
      }

      if (!done && chunk) {
        // Update assistant message with new chunk
        set(state => {
          const updatedOptimistic = new Map(state.optimisticMessages);
          const currentMsg = updatedOptimistic.get(assistantTempId);

          if (currentMsg) {
            const nextContent = currentMsg.content.startsWith(RETRYING_NOTICE)
              ? chunk
              : currentMsg.content + chunk;
            updatedOptimistic.set(assistantTempId, {
              ...currentMsg,
              content: nextContent,
            });
          }

          return { optimisticMessages: updatedOptimistic };
        });
      }

      if (done) {
        unlistenFn();
      }
    });

    // Call backend (this is where UI used to freeze)
    const result = await VaultAPI.chatWithConversation(
      requestConversationId,
      content,
      toolPreferences
    );

    if (!result.ok) {
      if (import.meta.env.DEV) {
        console.error('[ConversationsStore] chat_with_conversation FAILED', {
          activeConversationId: requestConversationId,
          error: result.error,
        });
      }

      // Cleanup listener on error
      unlistenFn();

      const errorCode = result.details?.code;
      if (isUserInitiatedCancellation(requestConversationId, errorCode)) {
        pendingCancellationConversations.delete(requestConversationId);
        const messagesAfterCancel = await VaultAPI.getConversationMessages(requestConversationId);
        const refreshedMessages = messagesAfterCancel.ok
          ? (Array.isArray(messagesAfterCancel.data)
            ? messagesAfterCancel.data
            : messagesAfterCancel.data.messages)
          : undefined;

        set((state) => {
          const cleanedOptimistic = new Map(state.optimisticMessages);
          cleanedOptimistic.delete(tempId);
          cleanedOptimistic.delete(assistantTempId);
          const conversations = refreshedMessages
            ? state.conversations.map((conv) =>
              conv.id === requestConversationId
                ? { ...conv, messages: refreshedMessages, updatedAt: new Date().toISOString() }
                : conv
            )
            : state.conversations;
          return {
            conversations,
            optimisticMessages: cleanedOptimistic,
            error: null,
            isSending: false,
          };
        });
        return;
      }

      pendingCancellationConversations.delete(requestConversationId);

      // FAILURE: Update optimistic message with error state
      const failedOptimisticMessages = new Map(get().optimisticMessages);
      const failedMsg = failedOptimisticMessages.get(tempId);

      if (failedMsg) {
        failedOptimisticMessages.set(tempId, {
          ...failedMsg,
          status: 'failed',
          error: result.error,
        });
      }

      // Remove assistant optimistic message on error
      failedOptimisticMessages.delete(assistantTempId);

      if (import.meta.env.DEV) {
        console.error('[ConversationsStore] Setting error state:', result.error);
      }

      set({
        optimisticMessages: failedOptimisticMessages,
        error: result.error,
        isSending: false,
      });
      return;
    }

    const rawResponse = result.data as {
      conversationId?: string;
      conversation_id?: string;
      messages?: Conversation['messages'];
      contextUsed?: number;
      context_used?: number;
      sources?: SourceWithMetadata[];
    };
    const responseConversationId = rawResponse.conversationId ?? rawResponse.conversation_id;
    const responseMessages = rawResponse.messages ?? [];
    const responseSources = rawResponse.sources ?? [];

    if (!responseConversationId) {
      set({
        error: 'Chat response missing conversation ID. Please try again.',
        isSending: false,
      });
      return;
    }

    if (import.meta.env.DEV) {
      console.log('[ConversationsStore] chat_with_conversation SUCCESS', {
        responseConversationId,
        messagesCount: responseMessages.length,
        contextUsed: rawResponse.contextUsed ?? rawResponse.context_used ?? 0,
      });
    }

    pendingCancellationConversations.delete(requestConversationId);

    // SUCCESS: Remove optimistic messages and update with real messages
    const updatedOptimisticMessages = new Map(get().optimisticMessages);
    updatedOptimisticMessages.delete(tempId);
    updatedOptimisticMessages.delete(assistantTempId);

    // Cleanup listener on success
    unlistenFn();

    const updatedConversations = get().conversations.map(conv =>
      conv.id === responseConversationId
        ? { ...conv, messages: responseMessages, updatedAt: new Date().toISOString() }
        : conv
    );

    let conversations = updatedConversations;
    const existingConv = conversations.find(c => c.id === responseConversationId);
    if (!existingConv) {
      conversations = [
        {
          id: responseConversationId,
          title: 'New Chat',
          messages: responseMessages,
          updatedAt: new Date().toISOString(),
        },
        ...conversations,
      ];
    }

    if (import.meta.env.DEV) {
      console.log('[ConversationsStore] Setting state with new conversation data', {
        conversationsCount: conversations.length,
        newActiveConversationId: responseConversationId,
        optimisticMessagesCount: updatedOptimisticMessages.size,
      });
    }

    // Store sources/verification keyed by assistant message ID for rendering.
    const updatedSources = new Map(get().lastMessageSources);
    const updatedVerification = new Map(get().messageVerification);
    const lastAssistantMsg = [...responseMessages].reverse().find((m) => m.role === 'assistant');
    if (lastAssistantMsg) {
      // Prefer persisted metadata first (includes richer structure and survives reload).
      if (lastAssistantMsg.metadata) {
        try {
          const parsedMetadata = JSON.parse(lastAssistantMsg.metadata) as Record<string, unknown>;
          const metadataSources = parsePersistedSources(parsedMetadata.sources);
          if (metadataSources.length > 0) {
            updatedSources.set(lastAssistantMsg.id, metadataSources);
          }
          const verification = parsePersistedVerification(parsedMetadata.verification);
          if (verification) {
            updatedVerification.set(lastAssistantMsg.id, verification);
          }
        } catch {
          // Ignore invalid metadata JSON.
        }
      }

      // Backward-compatibility fallback: use direct response sources payload.
      if (!updatedSources.has(lastAssistantMsg.id) && responseSources.length > 0) {
        const normalizedResponseSources = parsePersistedSources(responseSources);
        if (normalizedResponseSources.length > 0) {
          updatedSources.set(lastAssistantMsg.id, normalizedResponseSources);
        }
      }
    }

    set({
      conversations,
      activeConversationId: responseConversationId,
      optimisticMessages: updatedOptimisticMessages,
      lastMessageSources: updatedSources,
      messageVerification: updatedVerification,
      isSending: false,
    });

    void get().loadConversationLinkedDocuments(responseConversationId);

    if (import.meta.env.DEV) {
      console.log('[ConversationsStore] sendMessage COMPLETE', {
        activeConversationId: get().activeConversationId,
      });
    }
  },

  cancelGeneration: async (conversationId?: string | null) => {
    const targetId = conversationId ?? get().activeConversationId;
    if (!targetId) return;

    pendingCancellationConversations.add(targetId);
    const result = await VaultAPI.cancelConversationGeneration(targetId);
    if (!result.ok) {
      pendingCancellationConversations.delete(targetId);
      set({ error: result.error });
    }
  },

  deleteConversation: async (id: string) => {
    // Step 1: Save for rollback
    const conversationToDelete = get().conversations.find(c => c.id === id);
    const wasActive = get().activeConversationId === id;

    if (!conversationToDelete) {
      set({ error: `Conversation not found: ${id}` });
      return;
    }

    // Step 2: Optimistically update UI
    set(state => {
      const newState: Partial<ConversationsState> = {
        conversations: state.conversations.filter(c => c.id !== id)
      };

      // Clear active conversation if it's the one being deleted
      if (state.activeConversationId === id) {
        newState.activeConversationId = null;
      }

      return newState;
    });

    // Step 3: Call backend with error handling
    const result = await VaultAPI.deleteConversation(id);
    if (!result.ok) {
      // Rollback: restore conversation AND activeConversationId
      set(state => ({
        conversations: [...state.conversations, conversationToDelete]
          .sort((a, b) => new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime()),
        activeConversationId: wasActive ? id : state.activeConversationId,
        error: result.error
      }));
    }
  },

  clearError: () => set({ error: null }),
}));

/**
 * Selector: Get combined messages (real + optimistic) for a conversation
 *
 * Merges real messages from database with temporary optimistic messages.
 * Optimistic messages are shown until backend responds.
 *
 * @param conversationId - ID of conversation
 * @returns Array of DisplayMessage (real ConversationMessage or OptimisticMessage)
 */
export const getConversationMessages = (conversationId: string | null): DisplayMessage[] => {
  if (!conversationId) return [];

  const state = useConversationsStore.getState();
  const conversation = state.conversations.find(c => c.id === conversationId);
  const realMessages: DisplayMessage[] = conversation?.messages || [];

  // Get optimistic messages for this conversation
  const optimisticForConv = Array.from(state.optimisticMessages.values())
    .filter(msg => msg.conversationId === conversationId || msg.conversationId === 'temp');

  // Combine: real messages + optimistic messages
  // Optimistic messages are always at the end (most recent)
  return [...realMessages, ...optimisticForConv];
};
