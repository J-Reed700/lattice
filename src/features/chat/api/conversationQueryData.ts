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
    spaceId: params.spaceId ?? null,
    query: trimmedQuery || null,
    savedOnly: params.filterMode === 'saved' || null,
    bookmarkedOnly: params.filterMode === 'bookmarked' || null,
    pinnedOnly: params.filterMode === 'pinned' || null,
    hasMessageBookmarks: params.filterMode === 'snippets' || null,
    includeArchived: params.filterMode === 'archived',
    limit: null,
    offset: null,
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

function extractMetadataError(metadata: string | null | undefined): string | undefined {
  if (!metadata) return undefined;
  try {
    const parsed: unknown = JSON.parse(metadata);
    if (parsed && typeof parsed === 'object' && 'error' in parsed && typeof parsed.error === 'string') {
      const trimmed = parsed.error.trim();
      if (trimmed) return trimmed;
    }
  } catch {
    // Malformed historical metadata must not hide the message itself.
  }
  return undefined;
}

export function toConversationMessage(message: MessageDto): ConversationMessage {
  switch (message.status) {
    case 'pending': case 'processing': case 'completed': case 'failed': {
      const error = message.status === 'failed' ? extractMetadataError(message.metadata) : undefined;
      return { ...message, status: message.status, ...(error ? { error } : {}) };
    }
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
    query: query.trim() || null,
    limit: null,
    offset: null,
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
