import { useQuery } from '@tanstack/react-query';

import { VaultAPI } from '@/lib/api';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import type { ConversationDto, ConversationMessageBookmarkDto } from '@/types';

/**
 * Conversations and bookmarks matching the spotlight query, read while the
 * spotlight is open. A failed read shows as no results.
 */
const NO_CONVERSATIONS: ConversationDto[] = [];
const NO_BOOKMARKS: ConversationMessageBookmarkDto[] = [];

async function searchSpotlight(q: string) {
  const [conversationsResult, bookmarksResult] = await Promise.all([
    VaultAPI.listConversationsExplorer({
      query: q || undefined,
      includeArchived: true,
      limit: 20,
      offset: 0,
    }),
    VaultAPI.listMessageBookmarks({
      query: q || undefined,
      limit: 20,
      offset: 0,
    }),
  ]);

  return {
    conversations: conversationsResult.ok
      ? (Array.isArray(conversationsResult.data)
        ? NO_CONVERSATIONS
        // Explorer threads open in the Explorer, never in Chat.
        : conversationsResult.data.conversations.filter((conversation) => !conversation.explorerRoot))
      : NO_CONVERSATIONS,
    bookmarks: bookmarksResult.ok
      ? (Array.isArray(bookmarksResult.data)
        ? NO_BOOKMARKS
        : bookmarksResult.data.bookmarks)
      : NO_BOOKMARKS,
  };
}

export function useSpotlightSearch(debouncedQuery: string, isOpen: boolean) {
  const q = debouncedQuery.trim();
  const searchQuery = useQuery({
    queryKey: [...conversationKeys.all, 'spotlight', q] as const,
    queryFn: () => searchSpotlight(q),
    enabled: isOpen,
    // Opening the spotlight again shows the last answer while it is re-read.
    staleTime: 0,
  });

  return {
    isLoading: searchQuery.isLoading,
    conversations: searchQuery.data?.conversations ?? NO_CONVERSATIONS,
    bookmarks: searchQuery.data?.bookmarks ?? NO_BOOKMARKS,
  };
}
