import { VaultAPI } from '@/lib/api';
import type { MessageDto } from '@/lib/bindings';
import type {
  ApiResult, ConversationLinkedDocumentDto, ConversationSpaceDto,
  ConversationWebSourceDto, DocumentSpaceMembershipDto,
} from '@/types';
import type { Conversation, ConversationMessage, ConversationMessageBookmark } from '@/types/conversation';

import type { ConversationListParams } from '../queries/conversationKeys';

export const unwrap = <T,>(result: ApiResult<T>): T => {
  if (!result.ok) {
    throw new Error(result.error);
  }
  return result.data;
};

export const fetchSpaces = async (): Promise<ConversationSpaceDto[]> =>
  unwrap(await VaultAPI.listConversationSpaces());

export const fetchConversationList = async (params: ConversationListParams): Promise<Conversation[]> => {
  const trimmedQuery = params.searchQuery.trim();
  const requiresExplorerQuery = Boolean(
    params.spaceId ||
    trimmedQuery ||
    params.filterMode !== 'all'
  );
  const explorerResult = await VaultAPI.listConversationsExplorer({
    spaceId: params.spaceId ?? undefined,
    query: trimmedQuery || undefined,
    savedOnly: params.filterMode === 'saved' || undefined,
    bookmarkedOnly: params.filterMode === 'bookmarked' || undefined,
    pinnedOnly: params.filterMode === 'pinned' || undefined,
    hasMessageBookmarks: params.filterMode === 'snippets' || undefined,
    includeArchived: params.filterMode === 'archived',
  });

  if (!explorerResult.ok && requiresExplorerQuery) {
    throw new Error(explorerResult.error);
  }

  const result = explorerResult.ok ? explorerResult : await VaultAPI.listConversations();
  if (!result.ok) {
    throw new Error(result.error);
  }
  const conversations = result.data.conversations as Conversation[];
  return params.filterMode === 'archived'
    ? conversations.filter(conversation => Boolean(conversation.isArchived))
    : conversations;
};

export const fetchConversationDetail = async (id: string): Promise<Conversation | null> => {
  const result = await VaultAPI.getConversation(id);
  if (!result.ok) throw new Error(result.error);
  return result.data.conversation as Conversation | null;
};

export function toConversationMessage(message: MessageDto): ConversationMessage {
  switch (message.status) {
    case 'pending': case 'processing': case 'completed': case 'failed':
      return { ...message, status: message.status };
    default:
      throw new Error(`Unknown conversation message status: ${message.status}`);
  }
}

export const fetchMessages = async (id: string): Promise<ConversationMessage[]> => {
  const result = await VaultAPI.getConversationMessages(id);
  if (!result.ok) throw new Error(result.error);
  const messages = result.data.messages.map(toConversationMessage);
  return messages;
};

export const fetchBookmarks = async (id: string, query = ''): Promise<ConversationMessageBookmark[]> => {
  const result = await VaultAPI.listMessageBookmarks({
    conversationId: id,
    query: query.trim() || undefined,
    limit: 200,
    offset: 0,
  });
  if (!result.ok) throw new Error(result.error);
  return result.data.bookmarks as ConversationMessageBookmark[];
};

export const fetchLinkedDocuments = async (id: string): Promise<ConversationLinkedDocumentDto[]> =>
  unwrap(await VaultAPI.listConversationLinkedDocuments(id));

export const fetchWebSources = async (id: string): Promise<ConversationWebSourceDto[]> =>
  unwrap(await VaultAPI.listConversationWebSources(id));

export const fetchMemberships = async (documentId: string): Promise<DocumentSpaceMembershipDto[]> =>
  unwrap(await VaultAPI.listDocumentSpaceMemberships(documentId));
