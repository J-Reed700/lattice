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

  createConversation: async (
    title: string,
    systemPrompt?: string,
  ): Promise<ApiResult<CreateConversationResponse>> =>
    apiCall<Wire.CreateConversationResponseDto>('create_conversation', {
      request: {
        title,
        systemPrompt,
      },
    }),

  setConversationModelChoice: async (
    conversationId: string,
    choice: Wire.ChatModelChoice,
  ): Promise<ApiResult<Wire.ConversationDto>> =>
    apiCall<Wire.ConversationDto>('set_conversation_model_choice', {
      conversationId,
      choice,
    }),

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

  getConversation: async (
    conversationId: string,
  ): Promise<ApiResult<GetConversationResponse>> =>
    apiCall<Wire.GetConversationResponseDto>('get_conversation', {
      request: {
        conversationId,
      },
    }),

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

  chatWithConversation: async (
    conversationId: string | null,
    message: string,
    toolPreferences?: ToolPreferences,
    requestId?: string,
    attachmentNames?: string[],
    attachmentDocumentIds?: string[],
    attachmentImageIds?: string[],
  ): Promise<ApiResult<Wire.ChatResponse>> =>
    apiCall<Wire.ChatResponse>('chat_with_conversation', {
      conversationId,
      message,
      requestId,
      toolPreferences,
      attachmentNames,
      attachmentDocumentIds,
      attachmentImageIds,
    }),

  attachMessageImages: (conversationId: string, paths: string[]): Promise<ApiResult<Wire.ChatImageDto[]>> =>
    apiCall<Wire.ChatImageDto[]>('attach_message_images', { conversationId, paths }),

  releaseMessageImages: (conversationId: string, imageIds: string[]): Promise<ApiResult<null>> =>
    apiCall<null>('release_message_images', { conversationId, imageIds }),

  restageMessageImage: (conversationId: string, imageId: string): Promise<ApiResult<Wire.ChatImageDto>> =>
    apiCall<Wire.ChatImageDto>('restage_message_image', { conversationId, imageId }),

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

  deleteConversation: async (
    conversationId: string,
  ): Promise<ApiResult<DeleteConversationResponse>> =>
    apiCall<Wire.DeleteConversationResponseDto>('delete_conversation', {
      request: {
        conversationId,
      },
    }),

  createConversationSpace: async (
    request: CreateConversationSpaceRequest,
  ): Promise<ApiResult<ConversationSpaceDto>> =>
    apiCall<Wire.ConversationSpaceDto>('create_conversation_space', {
      request,
    }),

  listConversationSpaces: async (): Promise<
    ApiResult<ConversationSpaceDto[]>
  > => apiCall<Wire.ConversationSpaceDto[]>('list_conversation_spaces'),

  createJournal: async (
    request: CreateConversationJournalRequest,
  ): Promise<ApiResult<ConversationJournalDto>> =>
    apiCall<Wire.ConversationJournalDto>('create_journal', { request }),

  listJournals: async (): Promise<ApiResult<ConversationJournalDto[]>> =>
    apiCall<Wire.ConversationJournalDto[]>('list_journals'),

  listConversationSpaceMembers: async (
    spaceId: string,
  ): Promise<ApiResult<ConversationSpaceMemberDto[]>> =>
    apiCall<Wire.ConversationSpaceMemberDto[]>(
      'list_conversation_space_members',
      {
        spaceId,
      },
    ),

  upsertConversationSpaceMember: async (
    request: UpsertConversationSpaceMemberRequest,
  ): Promise<ApiResult<ConversationSpaceMemberDto>> =>
    apiCall<Wire.ConversationSpaceMemberDto>(
      'upsert_conversation_space_member',
      { request },
    ),

  removeConversationSpaceMember: async (
    request: RemoveConversationSpaceMemberRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>(
      'remove_conversation_space_member',
      { request },
    ),

  updateConversationSpace: async (
    request: UpdateConversationSpaceRequest,
  ): Promise<ApiResult<ConversationSpaceDto>> =>
    apiCall<Wire.ConversationSpaceDto>('update_conversation_space', {
      request,
    }),

  archiveConversationSpace: async (
    request: ArchiveConversationSpaceRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('archive_conversation_space', {
      request,
    }),

  updateJournal: async (
    request: UpdateConversationJournalRequest,
  ): Promise<ApiResult<ConversationJournalDto>> =>
    apiCall<Wire.ConversationJournalDto>('update_journal', { request }),

  archiveJournal: async (
    request: ArchiveConversationJournalRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('archive_journal', { request }),

  deleteJournal: async (
    request: DeleteConversationJournalRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('delete_journal', { request }),

  moveConversationToSpace: async (
    request: MoveConversationToSpaceRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('move_conversation_to_space', {
      request,
    }),

  addConversationToJournal: async (
    request: AddConversationToJournalRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('add_conversation_to_journal', {
      request,
    }),

  removeConversationFromJournal: async (
    request: RemoveConversationFromJournalRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>(
      'remove_conversation_from_journal',
      { request },
    ),

  setConversationSaved: async (
    request: SetConversationStateRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('set_conversation_saved', {
      request,
    }),

  setConversationBookmarked: async (
    request: SetConversationStateRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('set_conversation_bookmarked', {
      request,
    }),

  setConversationPinned: async (
    request: SetConversationStateRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('set_conversation_pinned', {
      request,
    }),

  setConversationArchived: async (
    request: SetConversationStateRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('set_conversation_archived', {
      request,
    }),

  deleteConversationMessage: async (
    request: DeleteConversationMessageRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('delete_conversation_message', {
      request,
    }),

  bookmarkConversationMessage: async (
    request: BookmarkConversationMessageRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>(
      'bookmark_conversation_message',
      { request },
    ),

  unbookmarkConversationMessage: async (
    request: UnbookmarkConversationMessageRequest,
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>(
      'unbookmark_conversation_message',
      { request },
    ),

  listMessageBookmarks: async (
    query?: ListMessageBookmarksQuery,
  ): Promise<ApiResult<ListMessageBookmarksResponse>> =>
    apiCall<Wire.ListMessageBookmarksResponseDto>('list_message_bookmarks', {
      query: query ?? {},
    }),

  listConversationsExplorer: async (
    query?: ListConversationsExplorerQuery,
  ): Promise<ApiResult<ListConversationsResponse>> =>
    apiCall<Wire.ListConversationsResponseDto>('list_conversations_explorer', {
      query: query ?? {},
    }),

  listJournalConversations: async (
    query: ListJournalConversationsQuery,
  ): Promise<ApiResult<ListConversationsResponse>> =>
    apiCall<Wire.ListConversationsResponseDto>('list_journal_conversations', {
      query,
    }),

  listJournalEntryPins: (
    journalSpaceId: string,
  ): Promise<ApiResult<string[]>> =>
    apiCall('list_journal_entry_pins', { journalSpaceId }),

  setJournalEntryPinned: (
    request: Wire.SetJournalEntryPinnedRequestDto,
  ): Promise<ApiResult<void>> =>
    apiCall('set_journal_entry_pinned', { request }),

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

  listConversationLinkedDocuments: async (
    conversationId: string,
  ): Promise<ApiResult<ConversationLinkedDocumentDto[]>> =>
    apiCall<Wire.ConversationLinkedDocumentDto[]>(
      'list_conversation_linked_documents',
      {
        conversationId,
      },
    ),

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

  listConversationWebSources: async (
    conversationId: string,
  ): Promise<ApiResult<ConversationWebSourceDto[]>> =>
    apiCall<Wire.ConversationWebSourceDto[]>('list_conversation_web_sources', {
      conversationId,
    }),

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

  listDocumentSpaceMemberships: async (
    documentId: string,
  ): Promise<ApiResult<DocumentSpaceMembershipDto[]>> =>
    apiCall<Wire.DocumentSpaceMembershipDto[]>(
      'list_document_space_memberships',
      {
        documentId,
      },
    ),

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

  forkConversation: async (
    conversationId: string,
    upToMessageId?: string,
  ): Promise<
    ApiResult<{ conversation: Conversation; copiedMessageCount: number }>
  > =>
    apiCall('fork_conversation', {
      request: { conversationId, upToMessageId },
    }),

  continueInNewConversation: async (
    conversationId: string,
  ): Promise<ApiResult<{ conversation: Conversation }>> =>
    apiCall('continue_in_new_conversation', {
      request: { conversationId },
    }),

  regenerateResponse: async (
    conversationId: string,
    toolPreferences?: ToolPreferences,
    requestId?: string,
    model?: Wire.ModelRef,
  ): Promise<ApiResult<Wire.ChatResponse>> =>
    apiCall<Wire.ChatResponse>('regenerate_response', {
      conversationId,
      toolPreferences,
      requestId,
      model: model ?? null,
    }),

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

  addDocumentsToLibrary: async (
    documentIds: string[],
  ): Promise<ApiResult<RenameConversationResponse>> =>
    apiCall<Wire.RenameConversationResponseDto>('add_documents_to_library', {
      documentIds,
    }),
};
