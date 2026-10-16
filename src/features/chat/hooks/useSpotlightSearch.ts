import { useEffect, useState } from 'react';

import { VaultAPI } from '@/lib/api';
import type { ConversationDto, ConversationMessageBookmarkDto } from '@/types';

/**
 * Conversations and bookmarks matching the spotlight query, read while the
 * spotlight is open. A failed read shows as no results.
 */
export function useSpotlightSearch(debouncedQuery: string, isOpen: boolean) {
  const [isLoading, setIsLoading] = useState(false);
  const [conversations, setConversations] = useState<ConversationDto[]>([]);
  const [bookmarks, setBookmarks] = useState<ConversationMessageBookmarkDto[]>([]);

  useEffect(() => {
    if (!isOpen) return;

    let cancelled = false;
    const q = debouncedQuery.trim();

    const run = async () => {
      setIsLoading(true);
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

      if (cancelled) return;

      setConversations(
        conversationsResult.ok
          ? (Array.isArray(conversationsResult.data)
            ? []
            // Explorer threads open in the Explorer, never in Chat.
            : conversationsResult.data.conversations.filter((conversation) => !conversation.explorerRoot))
          : []
      );

      setBookmarks(
        bookmarksResult.ok
          ? (Array.isArray(bookmarksResult.data)
            ? []
            : bookmarksResult.data.bookmarks)
          : []
      );

      setIsLoading(false);
    };

    void run();

    return () => {
      cancelled = true;
    };
  }, [debouncedQuery, isOpen]);

  return { isLoading, conversations, bookmarks };
}
