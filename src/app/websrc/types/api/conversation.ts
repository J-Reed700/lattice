/**
 * Conversation API Types
 *
 * Type definitions for conversation management operations.
 * These types match the Rust backend structures from application/dtos/modules/conversation_dto.rs
 */

/**
 * Conversation representation with metadata and usage statistics.
 */
export interface ConversationDto {
  /** Conversation identifier (UUID) */
  id: string;
  /** Conversation title */
  title: string;
  /** LLM model name (e.g., "claude-sonnet-4-5-20250929") */
  modelName: string;
  /** Optional system prompt */
  systemPrompt: string | null;
  /** Creation timestamp (ISO 8601) */
  createdAt: string;
  /** Last update timestamp (ISO 8601) */
  updatedAt: string;
  /** Number of messages in conversation */
  messageCount: number;
  /** Total tokens used across all messages */
  totalTokens: number;
  /** Space/environment identifier that owns this conversation */
  spaceId?: string | null;
  /** Whether this conversation is saved */
  isSaved?: boolean | null;
  /** Whether this conversation is bookmarked */
  isBookmarked?: boolean | null;
  /** Whether this conversation is pinned */
  isPinned?: boolean | null;
  /** Whether this conversation is archived */
  isArchived?: boolean | null;
  /** Timestamp when saved (ISO 8601) */
  savedAt?: string | null;
  /** Timestamp when bookmarked (ISO 8601) */
  bookmarkedAt?: string | null;
  /** Timestamp when pinned (ISO 8601) */
  pinnedAt?: string | null;
  /** Timestamp when archived (ISO 8601) */
  archivedAt?: string | null;
  /** Optional preview of latest message */
  lastMessagePreview?: string | null;
}

/**
 * Conversation message with role and content.
 */
export interface MessageDto {
  /** Message identifier (UUID) */
  id: string;
  /** Conversation identifier this message belongs to */
  conversationId: string;
  /** Message role (user, assistant, system) */
  role: 'user' | 'assistant' | 'system';
  /** Message content */
  content: string;
  /** Token count for this message */
  tokens: number;
  /** Creation timestamp (ISO 8601) */
  createdAt: string;
  /** Optional metadata (JSON string) */
  metadata: string | null;
  /** Message status (pending/completed/failed) */
  status: 'pending' | 'completed' | 'failed';
}

// Request DTOs

/**
 * Request to create a new conversation.
 */
export interface CreateConversationRequest {
  /** Conversation title */
  title: string;
  /** LLM model name */
  modelName: string;
  /** Optional system prompt */
  systemPrompt?: string;
}

/**
 * Request to get a conversation by ID.
 */
export interface GetConversationRequest {
  /** Conversation identifier */
  conversationId: string;
}

/**
 * Request to get conversation messages.
 */
export interface GetConversationMessagesRequest {
  /** Conversation identifier */
  conversationId: string;
}

/**
 * Request to rename a conversation.
 */
export interface RenameConversationRequest {
  /** Conversation identifier */
  conversationId: string;
  /** New title */
  newTitle: string;
}

/**
 * Request to delete a conversation.
 */
export interface DeleteConversationRequest {
  /** Conversation identifier */
  conversationId: string;
}

/**
 * Query parameters for listing conversations.
 */
export interface ListConversationsQuery {
  /** Maximum number of conversations to return */
  limit?: number;
  /** Offset for pagination */
  offset?: number;
}

// Response DTOs

/**
 * Response from creating a conversation.
 */
export interface CreateConversationResponse {
  /** The created conversation */
  conversation: ConversationDto;
  /** Status message */
  status: string;
}

/**
 * Response from getting a conversation.
 */
export interface GetConversationResponse {
  /** The conversation (null if not found) */
  conversation: ConversationDto | null;
}

/**
 * Response from getting conversation messages.
 */
export interface GetConversationMessagesResponse {
  /** List of messages in chronological order */
  messages: MessageDto[];
  /** Total count */
  total: number;
}

/**
 * Response from renaming a conversation.
 */
export interface RenameConversationResponse {
  /** Status message */
  status: string;
}

/**
 * Response from deleting a conversation.
 */
export interface DeleteConversationResponse {
  /** Status message */
  status: string;
}

/**
 * Response from listing conversations.
 */
export interface ListConversationsResponse {
  /** List of conversations */
  conversations: ConversationDto[];
  /** Total count (for pagination) */
  total: number;
}

/**
 * Request to synthesize multiple journal conversations into one structured summary.
 */
export interface SynthesizeJournalEntriesRequest {
  /** Ordered conversation IDs to include in synthesis */
  conversationIds: string[];
  /** Scope label for synthesis metadata */
  scope?: 'current' | 'deck' | 'pinned' | string;
  /** Optional cap for backend processing */
  maxEntries?: number;
}

/**
 * Response from journal synthesis command.
 */
export interface SynthesizeJournalEntriesResponse {
  /** Final synthesis markdown */
  synthesis: string;
  /** Scope echoed by backend */
  scope: string;
  /** Entries actually synthesized */
  entryCount: number;
  /** Number of map chunks processed */
  chunkCount: number;
  /** Conversation IDs used in synthesis */
  conversationIds: string[];
}

/**
 * Conversation space/environment DTO.
 */
export interface ConversationSpaceDto {
  id: string;
  name: string;
  description: string | null;
  icon: string | null;
  accentColor: string | null;
  spacePrompt: string | null;
  defaultModelName: string | null;
  toolPreferencesJson: string | null;
  isArchived: boolean;
  sortOrder: number;
  createdAt: string;
  updatedAt: string;
}

export interface ConversationJournalDto {
  id: string;
  name: string;
  description: string | null;
  icon: string | null;
  accentColor: string | null;
  spacePrompt: string | null;
  defaultModelName: string | null;
  toolPreferencesJson: string | null;
  isArchived: boolean;
  sortOrder: number;
  createdAt: string;
  updatedAt: string;
}

export interface CreateConversationSpaceRequest {
  name: string;
  description?: string | null;
  icon?: string | null;
  accentColor?: string | null;
  spacePrompt?: string | null;
  defaultModelName?: string | null;
  toolPreferencesJson?: string | null;
}

export interface CreateConversationJournalRequest {
  name: string;
  description?: string | null;
  icon?: string | null;
  accentColor?: string | null;
  spacePrompt?: string | null;
  defaultModelName?: string | null;
  toolPreferencesJson?: string | null;
}

export interface UpdateConversationSpaceRequest {
  spaceId: string;
  name?: string;
  description?: string | null;
  icon?: string | null;
  accentColor?: string | null;
  spacePrompt?: string | null;
  defaultModelName?: string | null;
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
  defaultModelName?: string | null;
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

export interface ConversationMessageBookmarkDto {
  id: string;
  conversationId: string;
  conversationTitle: string;
  spaceId: string;
  messageId: string;
  messageRole: 'user' | 'assistant' | 'system';
  messagePreview: string;
  title: string | null;
  note: string | null;
  createdAt: string;
}

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

export interface ConversationLinkedDocumentDto {
  documentId: string;
  fileName: string;
  filePath: string;
  fileType: string;
  category: string;
  indexedAt: string;
  lastReferencedAt: string;
  referenceCount: number;
}

export interface ConversationWebSourceDto {
  id: string;
  url: string;
  normalizedUrl: string;
  title?: string | null;
  excerpt?: string | null;
  relevanceScore?: number | null;
  addedAt: string;
}

export interface DocumentSpaceMembershipDto {
  spaceId: string;
  spaceName: string;
  isArchived: boolean;
  createdAt?: string | null;
}

export interface ConversationSpaceMemberDto {
  spaceId: string;
  memberId: string;
  displayName: string;
  email?: string | null;
  avatarUrl?: string | null;
  role: 'owner' | 'editor' | 'viewer';
  createdAt: string;
  updatedAt: string;
}

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
