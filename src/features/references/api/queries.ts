import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from '@tanstack/react-query';

import { VaultAPI } from '@/lib/api';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import type { ConversationMessageBookmarkDto } from '@/types';
import { unwrapApiResult } from '@/types/api/result';
import { resolveBookmarkPayload } from '@/utils/chatBookmarks';

export const bookmarkPayloadKey = (conversationId: string, messageId: string) =>
  ['references', 'bookmark-payload', conversationId, messageId] as const;

/**
 * A bookmarked message's full content and sources, read once when it is first
 * opened (a saved message does not change). A failed read is shown at once;
 * opening the reference again reads it again.
 */
export const bookmarkPayloadQueryOptions = (bookmark: ConversationMessageBookmarkDto | null) =>
  queryOptions({
    queryKey: bookmarkPayloadKey(bookmark?.conversationId ?? '', bookmark?.messageId ?? ''),
    queryFn: () => resolveBookmarkPayload(bookmark!),
    enabled: bookmark !== null,
    retry: false,
    staleTime: Infinity,
  });

export function useInboxBookmarksQuery(search: string) {
  const query = search.trim();
  return useQuery({
    queryKey: [...conversationKeys.allBookmarks, 'inbox', query],
    queryFn: async () =>
      unwrapApiResult(
        await VaultAPI.listMessageBookmarks({
          query: query || undefined,
          limit: 200,
          offset: 0,
        }),
      ).bookmarks,
  });
}

export async function saveBookmark(
  client: QueryClient,
  request: Parameters<typeof VaultAPI.bookmarkConversationMessage>[0],
) {
  const bookmark = unwrapApiResult(
    await VaultAPI.bookmarkConversationMessage(request),
  );
  await client.invalidateQueries({ queryKey: conversationKeys.allBookmarks });
  return bookmark;
}

export async function removeBookmark(
  client: QueryClient,
  request: Parameters<typeof VaultAPI.unbookmarkConversationMessage>[0],
) {
  unwrapApiResult(await VaultAPI.unbookmarkConversationMessage(request));
  await client.invalidateQueries({ queryKey: conversationKeys.allBookmarks });
}

export function useBookmarkMutations() {
  const client = useQueryClient();
  const save = useMutation({
    mutationFn: (request: Parameters<typeof saveBookmark>[1]) =>
      saveBookmark(client, request),
  });
  const remove = useMutation({
    mutationFn: (request: Parameters<typeof removeBookmark>[1]) =>
      removeBookmark(client, request),
  });
  return { save, remove };
}
