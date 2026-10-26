import { VaultAPI, type ApiClient } from '@/lib/api';
import type { MessageDto } from '@/lib/bindings';
import type { ConversationListParams } from '@/shared/conversations/conversationKeys';
import type {
  ApiResult, ConversationLinkedDocumentDto,
  ConversationWebSourceDto, DocumentSpaceMembershipDto,
} from '@/types';
import type { Conversation, ConversationMessage, ConversationMessageBookmark } from '@/types/conversation';


export const unwrap = <T,>(result: ApiResult<T>): T => {
  if (!result.ok) {
    throw new Error(result.error);
  }
  return result.data;
};

export const fetchConversationList = async (params: ConversationListParams, api: ApiClient = VaultAPI): Promise<Conversation[]> => {
  const trimmedQuery = params.searchQuery.trim();
  const requiresExplorerQuery = Boolean(
    params.spaceId ||
    trimmedQuery ||
    params.filterMode !== 'all'
  );
  const explorerResult = await api.listConversationsExplorer({
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

  const result = explorerResult.ok ? explorerResult : await api.listConversations();
  if (!result.ok) {
    throw new Error(result.error);
  }
  const conversations = result.data.conversations as Conversation[];
  return params.filterMode === 'archived'
    ? conversations.filter(conversation => Boolean(conversation.isArchived))
    : conversations;
};

export const fetchConversationDetail = async (id: string, api: ApiClient = VaultAPI): Promise<Conversation | null> => {
  const result = await api.getConversation(id);
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

export const fetchMessages = async (id: string, api: ApiClient = VaultAPI): Promise<ConversationMessage[]> => {
  const result = await api.getConversationMessages(id);
  if (!result.ok) throw new Error(result.error);
  const messages = result.data.messages.map(toConversationMessage);
  return messages;
};

export const fetchBookmarks = async (id: string, query = '', api: ApiClient = VaultAPI): Promise<ConversationMessageBookmark[]> => {
  const result = await api.listMessageBookmarks({
    conversationId: id,
    query: query.trim() || undefined,
    limit: 200,
    offset: 0,
  });
  if (!result.ok) throw new Error(result.error);
  return result.data.bookmarks as ConversationMessageBookmark[];
};

export const fetchLinkedDocuments = async (id: string, api: ApiClient = VaultAPI): Promise<ConversationLinkedDocumentDto[]> =>
  unwrap(await api.listConversationLinkedDocuments(id));

export const fetchWebSources = async (id: string, api: ApiClient = VaultAPI): Promise<ConversationWebSourceDto[]> =>
  unwrap(await api.listConversationWebSources(id));

export const fetchMemberships = async (documentId: string, api: ApiClient = VaultAPI): Promise<DocumentSpaceMembershipDto[]> =>
  unwrap(await api.listDocumentSpaceMemberships(documentId));
