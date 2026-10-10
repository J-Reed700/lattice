
import type * as Wire from '@/lib/bindings';
import { apiCall } from '@/shared/ipc/transport';
import type {
  ApiResult,
  Conversation,
  ToolPreferences,
  GetConversationMessagesResponse,
  CreateConversationResponse,
  ListConversationsResponse,
  GetConversationResponse,
  RenameConversationResponse,
  DeleteConversationResponse,
  ConversationSpaceDto,
  ConversationJournalDto,
  ConversationSpaceMemberDto,
  CreateConversationSpaceRequest,
  CreateConversationJournalRequest,
  UpdateConversationSpaceRequest,
  UpdateConversationJournalRequest,
  ArchiveConversationSpaceRequest,
  ArchiveConversationJournalRequest,
  DeleteConversationJournalRequest,
  MoveConversationToSpaceRequest,
  AddConversationToJournalRequest,
  RemoveConversationFromJournalRequest,
  UpsertConversationSpaceMemberRequest,
  RemoveConversationSpaceMemberRequest,
  SetConversationStateRequest,
  ListConversationsExplorerQuery,
  ListJournalConversationsQuery,
  DeleteConversationMessageRequest,
  BookmarkConversationMessageRequest,
  UnbookmarkConversationMessageRequest,
  ListMessageBookmarksQuery,
  ListMessageBookmarksResponse,
  ConversationLinkedDocumentDto,
  ConversationWebSourceDto,
  DocumentSpaceMembershipDto,
  SpaceDocument,
} from '@/types';

export const chatApi = {
  createConversationTangent: (request: Wire.CreateTangentRequestDto): Promise<ApiResult<Wire.ConversationTangentDto>> =>
    apiCall<Wire.ConversationTangentDto>('create_conversation_tangent', { request }),

  listConversationTangents: (conversationId: string): Promise<ApiResult<Wire.ConversationTangentDto[]>> =>
    apiCall<Wire.ConversationTangentDto[]>('list_conversation_tangents', { request: { conversationId } }),

  promoteConversationTangent: (conversationId: string): Promise<ApiResult<Wire.ConversationDto>> =>
    apiCall<Wire.ConversationDto>('promote_conversation_tangent', { request: { conversationId } }),

  /**
   * Creates a new conversation for multi-turn Q&A with context.
   * Initializes a conversation thread that maintains context across multiple
   * question-answering interactions.
   *
   * @param title - Human-readable conversation title
   * @param modelName - LLM model to use (e.g., "gpt-4", "claude-3-opus")
   * @param systemPrompt - Optional system instructions to guide LLM behavior
   * @returns Response with created conversation and status
   */
  createConversation: async (
    title: string,
    modelName: string,
    systemPrompt?: string,
  ): Promise<ApiResult<CreateConversationResponse>> =>
    apiCall<Wire.CreateConversationResponseDto>('create_conversation', {
      request: {
        title,
        modelName,
        systemPrompt,
      },
    }),

  /**
   * Lists all conversations with pagination support.
   * Returns conversations ordered by most recently updated first.
   *
   * @param limit - Maximum number of conversations to return (default: 100)
   * @param offset - Number of conversations to skip for pagination (default: 0)
   * @returns Response with conversations and total count
   */
  listConversations: async (
    limit?: number,
    offset?: number,
  ): Promise<ApiResult<ListConversationsResponse>> =>
    apiCall<Wire.ListConversationsResponseDto>('list_conversations', {
      query: {
        limit,
        offset,
      },
    }),

  /**
   * Retrieves a single conversation by ID.
   * Returns conversation metadata without loading message history.
   *
   * @param conversationId - Unique identifier of the conversation to retrieve
   * @returns Response with conversation metadata or null if not found
   */
  getConversation: async (
    conversationId: string,
  ): Promise<ApiResult<GetConversationResponse>> =>
    apiCall<Wire.GetConversationResponseDto>('get_conversation', {
      request: {
        conversationId,
      },
    }),

  /**
   * Retrieves all messages for a conversation.
   * Returns complete message history in chronological order.
   *
   * @param conversationId - Unique identifier of the conversation
   * @returns Response with messages and total count
   */
  getConversationMessages: async (
    conversationId: string,
  ): Promise<ApiResult<GetConversationMessagesResponse>> =>
    apiCall<Wire.GetConversationMessagesResponseDto>(
      'get_conversation_messages',
      {
        request: {
          conversationId,
        },
      },
    ),

  /**
   * Renames a conversation with a new title.
   * Updates the conversation's title (sanitized for security).
   *
   * @param conversationId - Unique identifier of the conversation to rename
   * @param newTitle - New title for the conversation
   * @returns Response with status message
   */
  renameConversation: async (
    conversationId: string,
    newTitle: string,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('rename_conversation', {
      request: {
        conversationId,
        newTitle,
      },
    }),

  /**
   * Sends a message to a conversation and receives an AI response.
   * Creates a new conversation if conversationId is null.
   *
   * @param conversationId - ID of existing conversation, or null to create new one
   * @param message - User's message text
   * @param attachmentNames - File names to stamp on the message, for the chips in history
   * @param attachmentDocumentIds - Documents this message brought in; the turn reads them whole
   * @returns Response containing conversation ID, all messages, and context usage
   */
  chatWithConversation: async (
    conversationId: string | null,
    message: string,
    toolPreferences?: ToolPreferences,
    requestId?: string,
    attachmentNames?: string[],
    attachmentDocumentIds?: string[],
  ): Promise<ApiResult<Wire.ChatResponse>> =>
    apiCall<Wire.ChatResponse>('chat_with_conversation', {
      conversationId,
      message,
      requestId,
      toolPreferences,
      attachmentNames,
      attachmentDocumentIds,
    }),

  /**
   * Cancels active model generation for a conversation if one is currently in-flight.
   */
  cancelConversationGeneration: async (
    conversationId: string,
    requestId: string,
  ): Promise<ApiResult<void>> => {
    const response = await apiCall<Wire.ChatResponse>(
      'chat_with_conversation',
      {
        conversationId,
        message: '',
        requestId,
        cancelOnly: true,
      },
    );
    if (!response.ok) {
      return response as unknown as ApiResult<void>;
    }
    return { ok: true, data: undefined };
  },

  /**
   * Deletes a conversation and all associated messages.
   * This operation is irreversible.
   *
   * @param conversationId - Unique identifier of the conversation to delete
   * @returns Response with status message
   */
  deleteConversation: async (
    conversationId: string,
  ): Promise<ApiResult<DeleteConversationResponse>> =>
    apiCall<Wire.DeleteConversationResponseDto>('delete_conversation', {
      request: {
        conversationId,
      },
    }),

  /**
   * Creates a conversation space (environment) with optional defaults.
   */
  createConversationSpace: async (
    request: CreateConversationSpaceRequest,
  ): Promise<ApiResult<ConversationSpaceDto>> =>
    apiCall<Wire.ConversationSpaceDto>('create_conversation_space', {
      request,
    }),

  /**
   * Lists all conversation spaces.
   */
  listConversationSpaces: async (): Promise<
    ApiResult<ConversationSpaceDto[]>
  > => apiCall<Wire.ConversationSpaceDto[]>('list_conversation_spaces'),

  /**
   * Creates a journal notebook.
   */
  createJournal: async (
    request: CreateConversationJournalRequest,
  ): Promise<ApiResult<ConversationJournalDto>> =>
    apiCall<Wire.ConversationJournalDto>('create_journal', { request }),

  /**
   * Lists all journals.
   */
  listJournals: async (): Promise<ApiResult<ConversationJournalDto[]>> =>
    apiCall<Wire.ConversationJournalDto[]>('list_journals'),

  /**
   * Lists members for a specific conversation space.
   */
  listConversationSpaceMembers: async (
    spaceId: string,
  ): Promise<ApiResult<ConversationSpaceMemberDto[]>> =>
    apiCall<Wire.ConversationSpaceMemberDto[]>(
      'list_conversation_space_members',
      {
        spaceId,
      },
    ),

  /**
   * Adds/updates a conversation space member and role.
   */
  upsertConversationSpaceMember: async (
    request: UpsertConversationSpaceMemberRequest,
  ): Promise<ApiResult<ConversationSpaceMemberDto>> =>
    apiCall<Wire.ConversationSpaceMemberDto>(
      'upsert_conversation_space_member',
      { request },
    ),

  /**
   * Removes a member from a conversation space.
   */
  removeConversationSpaceMember: async (
    request: RemoveConversationSpaceMemberRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>(
      'remove_conversation_space_member',
      { request },
    ),

  /**
   * Updates a conversation space.
   */
  updateConversationSpace: async (
    request: UpdateConversationSpaceRequest,
  ): Promise<ApiResult<ConversationSpaceDto>> =>
    apiCall<Wire.ConversationSpaceDto>('update_conversation_space', {
      request,
    }),

  /**
   * Archives/unarchives a conversation space.
   */
  archiveConversationSpace: async (
    request: ArchiveConversationSpaceRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('archive_conversation_space', {
      request,
    }),

  // The `*ConversationThread` wrappers that used to live here were removed:
  // the feature was renamed thread → space on the backend, so they invoked
  // `create_conversation_thread` and friends, none of which are registered
  // Tauri commands. Use the `*ConversationSpace` functions above.

  /**
   * Updates a journal notebook.
   */
  updateJournal: async (
    request: UpdateConversationJournalRequest,
  ): Promise<ApiResult<ConversationJournalDto>> =>
    apiCall<Wire.ConversationJournalDto>('update_journal', { request }),

  /**
   * Archives/unarchives a journal notebook.
   */
  archiveJournal: async (
    request: ArchiveConversationJournalRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('archive_journal', { request }),

  /**
   * Permanently deletes a journal notebook.
   */
  deleteJournal: async (
    request: DeleteConversationJournalRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('delete_journal', { request }),

  /**
   * Moves a conversation to a space.
   */
  moveConversationToSpace: async (
    request: MoveConversationToSpaceRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('move_conversation_to_space', {
      request,
    }),

  /**
   * Adds a conversation to a journal without changing its owning space.
   */
  addConversationToJournal: async (
    request: AddConversationToJournalRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('add_conversation_to_journal', {
      request,
    }),

  /**
   * Removes a conversation from a journal entry deck.
   */
  removeConversationFromJournal: async (
    request: RemoveConversationFromJournalRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>(
      'remove_conversation_from_journal',
      { request },
    ),

  /**
   * Sets the "saved" state for a conversation.
   */
  setConversationSaved: async (
    request: SetConversationStateRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('set_conversation_saved', {
      request,
    }),

  /**
   * Sets the "bookmarked" state for a conversation.
   */
  setConversationBookmarked: async (
    request: SetConversationStateRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('set_conversation_bookmarked', {
      request,
    }),

  /**
   * Sets the "pinned" state for a conversation.
   */
  setConversationPinned: async (
    request: SetConversationStateRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('set_conversation_pinned', {
      request,
    }),

  /**
   * Sets the "archived" state for a conversation.
   */
  setConversationArchived: async (
    request: SetConversationStateRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('set_conversation_archived', {
      request,
    }),

  /**
   * Deletes a specific message from a conversation.
   */
  deleteConversationMessage: async (
    request: DeleteConversationMessageRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('delete_conversation_message', {
      request,
    }),

  /**
   * Bookmarks a specific message in a conversation (upsert by message).
   */
  bookmarkConversationMessage: async (
    request: BookmarkConversationMessageRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>(
      'bookmark_conversation_message',
      { request },
    ),

  /**
   * Removes a message bookmark from a conversation.
   */
  unbookmarkConversationMessage: async (
    request: UnbookmarkConversationMessageRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>(
      'unbookmark_conversation_message',
      { request },
    ),

  /**
   * Lists message-level bookmarks with optional conversation and text filters.
   */
  listMessageBookmarks: async (
    query?: ListMessageBookmarksQuery,
  ): Promise<ApiResult<ListMessageBookmarksResponse>> =>
    apiCall<Wire.ListMessageBookmarksResponseDto>('list_message_bookmarks', {
      query: query ?? {},
    }),

  /**
   * Lists conversations for the explorer with filters and text search.
   */
  listConversationsExplorer: async (
    query?: ListConversationsExplorerQuery,
  ): Promise<ApiResult<ListConversationsResponse>> =>
    apiCall<Wire.ListConversationsResponseDto>('list_conversations_explorer', {
      query: query ?? {},
    }),

  /**
   * Lists conversations included in a journal (additive membership).
   */
  listJournalConversations: async (
    query: ListJournalConversationsQuery,
  ): Promise<ApiResult<ListConversationsResponse>> =>
    apiCall<Wire.ListConversationsResponseDto>('list_journal_conversations', {
      query,
    }),

  /** The conversations pinned in a journal, the most recently pinned first. */
  listJournalEntryPins: (
    journalSpaceId: string,
  ): Promise<ApiResult<string[]>> =>
    apiCall('list_journal_entry_pins', { journalSpaceId }),

  /** Pins or unpins an entry in one journal; the Chat sidebar's pin is separate. */
  setJournalEntryPinned: (
    request: Wire.SetJournalEntryPinnedRequestDto,
  ): Promise<ApiResult<void>> =>
    apiCall('set_journal_entry_pinned', { request }),

  /**
   * The documents a chat in this space may read, newest first.
   *
   * `spaceId` of `null` means General. The backend answers from the same
   * allow-list retrieval uses, so what this offers is exactly what a turn can
   * search — never a superset. `limit` is clamped to 50 there.
   */
  listSpaceDocuments: async (
    spaceId: string | null,
    conversationId: string | null,
    query: string,
    limit: number,
  ): Promise<ApiResult<SpaceDocument[]>> =>
    apiCall<Wire.SpaceDocumentDto[]>('list_space_documents', {
      spaceId,
      conversationId,
      query,
      limit,
    }),

  /**
   * Lists documents currently linked to a conversation context.
   */
  listConversationLinkedDocuments: async (
    conversationId: string,
  ): Promise<ApiResult<ConversationLinkedDocumentDto[]>> =>
    apiCall<Wire.ConversationLinkedDocumentDto[]>(
      'list_conversation_linked_documents',
      {
        conversationId,
      },
    ),

  /**
   * Removes a linked document reference from a conversation.
   */
  removeConversationLinkedDocument: async (
    conversationId: string,
    documentId: string,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>(
      'remove_conversation_linked_document',
      {
        conversationId,

        documentId,
      },
    ),

  /**
   * Adds or updates a non-ingested web source linked to a conversation.
   */
  addConversationWebSource: async (
    conversationId: string,
    url: string,
    options?: {
      title?: string;
      excerpt?: string;
      relevanceScore?: number;
    },
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('add_conversation_web_source', {
      conversationId,
      url,
      title: options?.title,
      excerpt: options?.excerpt,

      relevanceScore: options?.relevanceScore,
    }),

  /**
   * Lists non-ingested web sources currently linked to a conversation context.
   */
  listConversationWebSources: async (
    conversationId: string,
  ): Promise<ApiResult<ConversationWebSourceDto[]>> =>
    apiCall<Wire.ConversationWebSourceDto[]>('list_conversation_web_sources', {
      conversationId,
    }),

  /**
   * Removes a linked web source from a conversation context.
   */
  removeConversationWebSource: async (
    conversationId: string,
    sourceId: string,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>(
      'remove_conversation_web_source',
      {
        conversationId,

        sourceId,
      },
    ),

  /**
   * Lists all spaces currently assigned to a document.
   */
  listDocumentSpaceMemberships: async (
    documentId: string,
  ): Promise<ApiResult<DocumentSpaceMembershipDto[]>> =>
    apiCall<Wire.DocumentSpaceMembershipDto[]>(
      'list_document_space_memberships',
      {
        documentId,
      },
    ),

  /**
   * Assigns or removes a document from a space scope.
   */
  setDocumentSpaceMembership: async (
    documentId: string,
    spaceId: string,
    assigned: boolean,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>(
      'set_document_space_membership',
      {
        documentId,

        spaceId,
        assigned,
      },
    ),

  /**
   * Deletes every message after `messageId` (and `messageId` itself when
   * `inclusive`). Returns the remaining messages so callers can replace their
   * cache rather than reason about what was removed.
   */
  truncateConversationAfter: async (
    conversationId: string,
    messageId: string,
    inclusive = false,
  ): Promise<
    ApiResult<{
      conversationId: string;
      deletedCount: number;
      messages: Wire.MessageDto[];
    }>
  > =>
    apiCall('truncate_conversation_after', {
      request: { conversationId, messageId, inclusive },
    }),

  /** Creates a sibling conversation copying messages up to `upToMessageId`. */
  forkConversation: async (
    conversationId: string,
    upToMessageId?: string,
  ): Promise<
    ApiResult<{ conversation: Conversation; copiedMessageCount: number }>
  > =>
    apiCall('fork_conversation', {
      request: { conversationId, upToMessageId },
    }),

  /**
   * Summarizes a conversation and opens a new one in the same space whose
   * first message is that summary. One or more full generations: minutes on
   * a local model.
   */
  continueInNewConversation: async (
    conversationId: string,
  ): Promise<ApiResult<{ conversation: Conversation }>> =>
    apiCall('continue_in_new_conversation', {
      request: { conversationId },
    }),

  /**
   * Re-runs the last user message. Streams over `llm-stream` exactly like
   * `chatWithConversation`; the user message is not duplicated.
   */
  regenerateResponse: async (
    conversationId: string,
    toolPreferences?: ToolPreferences,
    requestId?: string,
  ): Promise<ApiResult<Wire.ChatResponse>> =>
    apiCall<Wire.ChatResponse>('regenerate_response', {
      conversationId,
      toolPreferences,
      requestId,
    }),

  /**
   * Folds the conversation's oldest messages into an LLM summary. The raw
   * messages stay in the history; only the LLM context switches to the
   * summary. Returns the applied compaction record.
   */
  compactConversation: async (
    conversationId: string,
    keepRecentMessages?: number,
  ): Promise<ApiResult<{ compaction: Wire.CompactionRecordDto }>> =>
    apiCall('compact_conversation', {
      request: {
        conversationId,
        keepRecentMessages: keepRecentMessages ?? null,
      },
    }),

  /**
   * Reads what the bounded memory layer currently holds for one conversation:
   * the active items with the quotations behind them, the counts, and the
   * `mode` that says whether any of it is usable.
   *
   * This read view resolves original quotations. manageKnowledge preserves
   * new user-authored source messages for additions and corrections.
   *
   * `includeHistory` also returns superseded and resolved items, for "what was
   * my original budget?".
   */
  manageKnowledge: async (
    request: Wire.KnowledgeRequestDto,
  ): Promise<ApiResult<Wire.KnowledgeResponseDto>> =>
    apiCall<Wire.KnowledgeResponseDto>('manage_knowledge', { request }),

  getConversationMemory: async (
    conversationId: string,
    includeHistory?: boolean,
  ): Promise<ApiResult<Wire.ConversationMemoryDetailsDto>> =>
    apiCall<Wire.ConversationMemoryDetailsDto>('get_conversation_memory', {
      request: { conversationId, includeHistory: includeHistory ?? null },
    }),

  /**
   * Assigns or removes multiple documents from a space scope.
   */
  setDocumentsSpaceMembership: async (
    documentIds: string[],
    spaceId: string,
    assigned: boolean,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>(
      'set_documents_space_membership',
      {
        documentIds,

        spaceId,
        assigned,
      },
    ),

  /**
   * Files a chat's attachments in the library.
   *
   * An attached file belongs to the conversation it arrived in: it is not
   * listed in the library, no other chat can search it, and it is deleted with
   * the conversation. This is the one way out of that — afterwards it is an
   * ordinary document. Documents that were never attachments are untouched.
   */
  addDocumentsToLibrary: async (
    documentIds: string[],
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('add_documents_to_library', {
      documentIds,
    }),
};
