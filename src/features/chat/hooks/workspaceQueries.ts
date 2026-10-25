import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { createJournal } from '@/features/journal/api/queries';
import type { VaultAPI } from '@/lib/api';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import type { ApiResult } from '@/types';

export { useJournalsQuery, useCapturedReferencesQuery, JOURNALS_QUERY_KEY } from '@/features/journal/api/queries';

export const SIDEBAR_BOOKMARKS_QUERY_KEY = conversationKeys.sidebarBookmarks;

function value<T>(result: ApiResult<T>): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}
export function useCreateJournalMutation() {
  const api = useApi();
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (request: Parameters<typeof VaultAPI.createJournal>[0]) => value(await createJournal(client, request, api)),
  });
}
export function useAddConversationsToJournalMutation() {
  const api = useApi();
  const client = useQueryClient();
  return useMutation({
    mutationFn: async ({ ids, journalId }: { ids: string[]; journalId: string }) => Promise.all(ids.map(async conversationId => ({
      conversationId, result: await api.addConversationToJournal({ journalSpaceId: journalId, conversationId }),
    }))),
    onSuccess: () => client.invalidateQueries({ queryKey: conversationKeys.all }),
  });
}
export function useSidebarBookmarksQuery(query: string, spaceId: string | null, enabled = true) {
  const api = useApi();
  return useQuery({
    queryKey: [...SIDEBAR_BOOKMARKS_QUERY_KEY, query, spaceId],
    enabled,
    queryFn: async () => {
      const response = value(await api.listMessageBookmarks({ query: query.trim() || undefined, limit: 60, offset: 0 }));
      const bookmarks = Array.isArray(response) ? [] : response.bookmarks;
      return spaceId ? bookmarks.filter(bookmark => bookmark.spaceId === spaceId) : bookmarks;
    },
  });
}
