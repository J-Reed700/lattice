/**
 * Conversation API Types
 *
 * Type definitions for conversation management operations.
 * These types match the Rust backend structures from application/dtos/modules/conversation_dto.rs
 */

/**
 * Conversation representation with metadata and usage statistics.
 */
export type ConversationDto = import('../../lib/bindings').ConversationDto;

/**
 * Conversation message with role and content.
 */
export type MessageDto = import('../../lib/bindings').MessageDto;

/**
 * Query parameters for listing conversations.
 */
export type ListConversationsQuery = import('../../lib/bindings').ListConversationsQuery;

export type CreateConversationResponse = import('../../lib/bindings').CreateConversationResponseDto;

export type GetConversationResponse = import('../../lib/bindings').GetConversationResponseDto;

export type GetConversationMessagesResponse = import('../../lib/bindings').GetConversationMessagesResponseDto;

export type RenameConversationResponse = import('../../lib/bindings').RenameConversationResponseDto;

export type DeleteConversationResponse = import('../../lib/bindings').DeleteConversationResponseDto;

/**
 * Response from listing conversations.
 */
export type ListConversationsResponse = import('../../lib/bindings').ListConversationsResponseDto;

/**
 * One source a synthesis drew on.
 */
export type SynthesisCitationDto = import('../../lib/bindings').SynthesisCitationDto;

/**
 * Conversation space/environment DTO.
 */
export type ConversationSpaceDto = import('../../lib/bindings').ConversationSpaceDto;

export type ConversationJournalDto = import('../../lib/bindings').ConversationJournalDto;

export type CreateConversationSpaceRequest = import('../../lib/bindings').CreateConversationSpaceRequestDto;

export type CreateConversationJournalRequest = import('../../lib/bindings').CreateConversationJournalRequestDto;

export type UpdateConversationSpaceRequest = import('../../lib/bindings').UpdateConversationSpaceRequestDto;

export type UpdateConversationJournalRequest = import('../../lib/bindings').UpdateConversationJournalRequestDto;

export type ArchiveConversationSpaceRequest = import('../../lib/bindings').ArchiveConversationSpaceRequestDto;

export type ArchiveConversationJournalRequest = import('../../lib/bindings').ArchiveConversationJournalRequestDto;

export type DeleteConversationJournalRequest = import('../../lib/bindings').DeleteConversationJournalRequestDto;

export type MoveConversationToSpaceRequest = import('../../lib/bindings').MoveConversationToSpaceRequestDto;

export type AddConversationToJournalRequest = import('../../lib/bindings').AddConversationToJournalRequestDto;

export type RemoveConversationFromJournalRequest = import('../../lib/bindings').RemoveConversationFromJournalRequestDto;

export type SetConversationStateRequest = import('../../lib/bindings').SetConversationStateRequestDto;

export type ListConversationsExplorerQuery = import('../../lib/bindings').ListConversationsExplorerQueryDto;

export type ListJournalConversationsQuery = import('../../lib/bindings').ListJournalConversationsQueryDto;

export type ConversationMessageBookmarkDto = import('../../lib/bindings').ConversationMessageBookmarkDto;

export type BookmarkConversationMessageRequest = import('../../lib/bindings').BookmarkConversationMessageRequestDto;

export type UnbookmarkConversationMessageRequest = import('../../lib/bindings').UnbookmarkConversationMessageRequestDto;

export type DeleteConversationMessageRequest = import('../../lib/bindings').DeleteConversationMessageRequestDto;

export type ListMessageBookmarksQuery = import('../../lib/bindings').ListMessageBookmarksQueryDto;

export type ListMessageBookmarksResponse = import('../../lib/bindings').ListMessageBookmarksResponseDto;

export type ConversationLinkedDocumentDto = import('../../lib/bindings').ConversationLinkedDocumentDto;

export type ConversationWebSourceDto = import('../../lib/bindings').ConversationWebSourceDto;

export type DocumentSpaceMembershipDto = import('../../lib/bindings').DocumentSpaceMembershipDto;

export type ConversationSpaceMemberDto = import('../../lib/bindings').ConversationSpaceMemberDto;

export type UpsertConversationSpaceMemberRequest = import('../../lib/bindings').UpsertConversationSpaceMemberRequestDto;

export type RemoveConversationSpaceMemberRequest = import('../../lib/bindings').RemoveConversationSpaceMemberRequestDto;
