import type { ConversationsState } from '@/stores/conversationsStore.types';

export interface ConversationListParams {
  spaceId: string | null;
  filterMode: ConversationsState['filterMode'];
  searchQuery: string;
}

export const conversationKeys = {
  all: ['conversations'] as const,
  lists: ['conversations', 'list'] as const,
  list: (params: ConversationListParams) => ['conversations', 'list', params] as const,
  spaces: ['conversations', 'spaces'] as const,
  detail: (id: string) => ['conversations', 'detail', id] as const,
  messages: (id: string) => ['conversations', 'messages', id] as const,
  allBookmarks: ['conversations', 'bookmarks'] as const,
  sidebarBookmarks: ['conversations', 'bookmarks', 'sidebar'] as const,
  bookmarks: (id: string, query = '') => ['conversations', 'bookmarks', id, query] as const,
  linkedDocuments: (id: string) => ['conversations', 'linked-documents', id] as const,
  webSources: (id: string) => ['conversations', 'web-sources', id] as const,
  memberships: (documentId: string) => ['conversations', 'memberships', documentId] as const,
};
