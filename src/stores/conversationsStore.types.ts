import type {
  ConversationLinkedDocumentDto,
  ConversationSpaceDto,
  ConversationWebSourceDto,
  DocumentSpaceMembershipDto,
} from '../types';
import type {
  CompactionRecord,
  Conversation,
  ConversationMessageBookmark,
  MessageVerificationSummary,
  OptimisticMessage,
  RetrievalTrace,
  SourceWithMetadata,
  ToolPreferences,
  TurnRecord,
  TurnStep,
} from '../types/conversation';

/**
 * How one generation turn ended.
 *
 * `busy` and `cancelled` are neither success nor failure: nothing was answered,
 * but nothing went wrong and the question was never taken away from the reader.
 * Callers must not announce an answer, or an error, for either.
 */
export type GenerationOutcome = 'answered' | 'busy' | 'cancelled' | 'failed';

export type ConversationFilterMode =
  | 'all'
  | 'saved'
  | 'bookmarked'
  | 'pinned'
  | 'archived'
  | 'snippets';

export interface LoadConversationsOverrides {
  spaceId?: string | null;
  filterMode?: ConversationFilterMode;
  searchQuery?: string;
}

export interface LoadMessageBookmarksOverrides {
  conversationId?: string;
  query?: string;
}

export interface ConversationsState {
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
  inFlightGenerations: Map<string, string>;
  error: string | null;
  optimisticMessages: Map<string, OptimisticMessage>;
  lastMessageSources: Map<string, SourceWithMetadata[]>;
  messageVerification: Map<string, MessageVerificationSummary>;
  /** Persisted retrieval trace keyed by assistant message id. */
  messageRetrieval: Map<string, RetrievalTrace>;
  /**
   * The persisted turn record keyed by assistant message id.
   *
   * Absent for every answer written before the record existed. Absent means
   * "not recorded", never a turn that did nothing.
   */
  messageTurn: Map<string, TurnRecord>;
  /** Live retrieval trace per conversation id, while a turn is in flight. */
  liveRetrieval: Map<string, RetrievalTrace>;
  /** What the in-flight turn has done so far, per conversation id. */
  liveSteps: Map<string, TurnStep[]>;
  /** Text the composer should adopt on its next render. */
  composerDraft: string | null;
  linkedDocumentsByConversationId: Map<string, ConversationLinkedDocumentDto[]>;
  webSourcesByConversationId: Map<string, ConversationWebSourceDto[]>;
  documentSpaceMembershipsByDocumentId: Map<string, DocumentSpaceMembershipDto[]>;

  loadSpaces: () => Promise<void>;
  setSelectedSpace: (_spaceId: string | null) => void;
  setFilterMode: (_mode: ConversationFilterMode) => void;
  setSearchQuery: (_query: string) => void;
  loadConversations: (_overrides?: LoadConversationsOverrides) => Promise<void>;
  loadMessageBookmarks: (_overrides?: LoadMessageBookmarksOverrides) => Promise<void>;
  /**
   * `_spaceId` files the chat into that space instead of the one selected in
   * the sidebar — for a chat made on behalf of another chat, whose space is
   * the only right answer.
   */
  createConversation: (_title: string, _spaceId?: string | null) => Promise<string>;
  setConversationSaved: (_id: string, _value: boolean) => Promise<void>;
  setConversationBookmarked: (_id: string, _value: boolean) => Promise<void>;
  setConversationPinned: (_id: string, _value: boolean) => Promise<void>;
  setConversationArchived: (_id: string, _value: boolean) => Promise<void>;
  renameConversation: (_id: string, _title: string) => Promise<boolean>;
  bookmarkMessage: (
    _conversationId: string,
    _messageId: string,
    _title?: string | null,
    _note?: string | null
  ) => Promise<void>;
  unbookmarkMessage: (_conversationId: string, _messageId: string) => Promise<void>;
  moveConversationToSpace: (_id: string, _spaceId: string) => Promise<void>;
  loadConversationLinkedDocuments: (_conversationId: string) => Promise<void>;
  loadConversationWebSources: (_conversationId: string) => Promise<void>;
  addConversationWebSource: (
    _conversationId: string,
    _url: string,
    _options?: { title?: string; excerpt?: string; relevanceScore?: number }
  ) => Promise<boolean>;
  removeConversationWebSource: (_conversationId: string, _sourceId: string) => Promise<boolean>;
  removeConversationLinkedDocument: (_conversationId: string, _documentId: string) => Promise<void>;
  loadDocumentSpaceMemberships: (_documentId: string) => Promise<void>;
  setDocumentSpaceMembership: (
    _documentId: string,
    _spaceId: string,
    _assigned: boolean
  ) => Promise<void>;
  selectConversation: (_id: string) => Promise<void>;
  sendMessage: (
    _content: string,
    _conversationId?: string | null,
    _toolPreferences?: ToolPreferences
  ) => Promise<void>;
  /**
   * Re-run the last user message. Resolves with how the turn ended; only
   * `'answered'` means an answer arrived, and only `'failed'` puts the question
   * back in the composer.
   */
  regenerateResponse: (
    _conversationId: string,
    _toolPreferences?: ToolPreferences
  ) => Promise<GenerationOutcome>;
  truncateAfter: (
    _conversationId: string,
    _messageId: string,
    _inclusive?: boolean
  ) => Promise<boolean>;
  forkConversation: (
    _conversationId: string,
    _upToMessageId?: string
  ) => Promise<string | null>;
  /**
   * Fold the conversation's oldest messages into an LLM summary so the context
   * window carries the distilled past. Resolves with the applied compaction
   * record, or `null` on failure (surfaced via the store's `error`).
   */
  compactConversation: (
    _conversationId: string,
    _keepRecentMessages?: number
  ) => Promise<CompactionRecord | null>;
  setComposerDraft: (_draft: string | null) => void;
  cancelGeneration: (_conversationId?: string | null) => Promise<void>;
  deleteMessage: (_conversationId: string, _messageId: string) => Promise<void>;
  deleteConversation: (_id: string) => Promise<void>;
  clearError: () => void;
}
