import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { VaultAPI, type ApiClient } from '@/lib/api';
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
export const bookmarkPayloadQueryOptions = (bookmark: ConversationMessageBookmarkDto | null, api: ApiClient = VaultAPI) =>
  queryOptions({
    queryKey: bookmarkPayloadKey(bookmark?.conversationId ?? '', bookmark?.messageId ?? ''),
    queryFn: () => resolveBookmarkPayload(bookmark!, undefined, api),
    enabled: bookmark !== null,
    retry: false,
    staleTime: Infinity,
  });

export function useInboxBookmarksQuery(search: string) {
  const api = useApi();
  const query = search.trim();
  return useQuery({
    queryKey: [...conversationKeys.allBookmarks, 'inbox', query],
    queryFn: async () =>
      unwrapApiResult(
        await api.listMessageBookmarks({
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
  api: ApiClient = VaultAPI,
) {
  const bookmark = unwrapApiResult(
    await api.bookmarkConversationMessage(request),
  );
  await client.invalidateQueries({ queryKey: conversationKeys.allBookmarks });
  return bookmark;
}

export async function removeBookmark(
  client: QueryClient,
  request: Parameters<typeof VaultAPI.unbookmarkConversationMessage>[0],
  api: ApiClient = VaultAPI,
) {
  unwrapApiResult(await api.unbookmarkConversationMessage(request));
  await client.invalidateQueries({ queryKey: conversationKeys.allBookmarks });
}

export function useBookmarkMutations() {
  const api = useApi();
  const client = useQueryClient();
  const save = useMutation({
    mutationFn: (request: Parameters<typeof saveBookmark>[1]) =>
      saveBookmark(client, request, api),
  });
  const remove = useMutation({
    mutationFn: (request: Parameters<typeof removeBookmark>[1]) =>
      removeBookmark(client, request, api),
  });
  return { save, remove };
}
