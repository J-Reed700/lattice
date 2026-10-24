export type ConversationDto = import('../../lib/bindings').ConversationDto;
export type MessageDto = import('../../lib/bindings').MessageDto;

export interface CreateConversationRequest {
  title: string;
  systemPrompt?: string;
}

export interface GetConversationRequest {
  conversationId: string;
}

export interface GetConversationMessagesRequest {
  conversationId: string;
}

export interface RenameConversationRequest {
  conversationId: string;
  newTitle: string;
}

export interface DeleteConversationRequest {
  conversationId: string;
}

export type ListConversationsQuery = import('../../lib/bindings').ListConversationsQuery;

export interface CreateConversationResponse {
  conversation: ConversationDto;
  status: string;
}

export interface GetConversationResponse {
  conversation: ConversationDto | null;
}

export interface GetConversationMessagesResponse {
  messages: MessageDto[];
  total: number;
}

export interface RenameConversationResponse {
  status: string;
}

export interface DeleteConversationResponse {
  status: string;
}

export interface ListConversationsResponse {
  conversations: ConversationDto[];
  total: number;
}

export type SynthesisCitationDto = import('../../lib/bindings').SynthesisCitationDto;
export type ConversationSpaceDto = import('../../lib/bindings').ConversationSpaceDto;

export type ConversationJournalDto = import('../../lib/bindings').ConversationJournalDto;

export interface CreateConversationSpaceRequest {
  name: string;
  description?: string | null;
  icon?: string | null;
  accentColor?: string | null;
  spacePrompt?: string | null;
  toolPreferencesJson?: string | null;
}

export interface CreateConversationJournalRequest {
  name: string;
  description?: string | null;
  icon?: string | null;
  accentColor?: string | null;
  spacePrompt?: string | null;
  toolPreferencesJson?: string | null;
}

export interface UpdateConversationSpaceRequest {
  spaceId: string;
  name?: string;
  description?: string | null;
  icon?: string | null;
  accentColor?: string | null;
  spacePrompt?: string | null;
  toolPreferencesJson?: string | null;
  isArchived?: boolean;
  sortOrder?: number;
}

export interface UpdateConversationJournalRequest {
  journalId: string;
  name?: string;
  description?: string | null;
  icon?: string | null;
  accentColor?: string | null;
  spacePrompt?: string | null;
  toolPreferencesJson?: string | null;
  isArchived?: boolean;
  sortOrder?: number;
}

export interface ArchiveConversationSpaceRequest {
  spaceId: string;
  archived: boolean;
}

export interface ArchiveConversationJournalRequest {
  journalId: string;
  archived: boolean;
}

export interface DeleteConversationJournalRequest {
  journalId: string;
}

export interface MoveConversationToSpaceRequest {
  conversationId: string;
  spaceId: string;
}

export interface AddConversationToJournalRequest {
  journalSpaceId: string;
  conversationId: string;
}

export interface RemoveConversationFromJournalRequest {
  journalSpaceId: string;
  conversationId: string;
}

export interface SetConversationStateRequest {
  conversationId: string;
  value: boolean;
}

export interface ListConversationsExplorerQuery {
  spaceId?: string;
  query?: string;
  savedOnly?: boolean;
  bookmarkedOnly?: boolean;
  pinnedOnly?: boolean;
  hasMessageBookmarks?: boolean;
  includeArchived?: boolean;
  limit?: number;
  offset?: number;
}

export interface ListJournalConversationsQuery {
  journalSpaceId: string;
  query?: string;
  includeArchived?: boolean;
  limit?: number;
  offset?: number;
}

export type ConversationMessageBookmarkDto = import('../../lib/bindings').ConversationMessageBookmarkDto;

export interface BookmarkConversationMessageRequest {
  conversationId: string;
  messageId: string;
  title?: string | null;
  note?: string | null;
}

export interface UnbookmarkConversationMessageRequest {
  conversationId: string;
  messageId: string;
}

export interface DeleteConversationMessageRequest {
  conversationId: string;
  messageId: string;
}

export interface ListMessageBookmarksQuery {
  conversationId?: string;
  query?: string;
  limit?: number;
  offset?: number;
}

export interface ListMessageBookmarksResponse {
  bookmarks: ConversationMessageBookmarkDto[];
  total: number;
}

export type ConversationLinkedDocumentDto = import('../../lib/bindings').ConversationLinkedDocumentDto;

export type ConversationWebSourceDto = import('../../lib/bindings').ConversationWebSourceDto;

export type DocumentSpaceMembershipDto = import('../../lib/bindings').DocumentSpaceMembershipDto;

export type ConversationSpaceMemberDto = import('../../lib/bindings').ConversationSpaceMemberDto;

export interface UpsertConversationSpaceMemberRequest {
  spaceId: string;
  memberId: string;
  displayName?: string;
  email?: string | null;
  avatarUrl?: string | null;
  role: 'owner' | 'editor' | 'viewer';
}

export interface RemoveConversationSpaceMemberRequest {
  spaceId: string;
  memberId: string;
}
