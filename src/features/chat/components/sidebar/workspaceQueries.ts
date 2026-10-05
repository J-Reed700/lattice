import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { createJournal } from '@/features/journal/api/queries';
import { buildSynthesisBlock } from '@/features/journal/model/synthesisTargets';
import { conversationKeys } from '@/hooks/queries/conversationKeys';
import { VaultAPI } from '@/lib/api';
import type { ApiResult } from '@/types';

export { useJournalsQuery, useCapturedReferencesQuery, JOURNALS_QUERY_KEY } from '@/features/journal/api/queries';

export const SIDEBAR_BOOKMARKS_QUERY_KEY = conversationKeys.sidebarBookmarks;

function value<T>(result: ApiResult<T>): T {
  if (!result.ok) throw new Error(result.error);
  return result.data;
}
export function useCreateJournalMutation() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (request: Parameters<typeof VaultAPI.createJournal>[0]) => value(await createJournal(client, request)),
  });
}
export function useAddConversationsToJournalMutation() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async ({ ids, journalId }: { ids: string[]; journalId: string }) => Promise.all(ids.map(async conversationId => ({
      conversationId, result: await VaultAPI.addConversationToJournal({ journalSpaceId: journalId, conversationId }),
    }))),
    onSuccess: () => client.invalidateQueries({ queryKey: conversationKeys.all }),
  });
}
export function useSpaceMutations() {
  const client = useQueryClient();
  const invalidate = () => client.invalidateQueries({ queryKey: conversationKeys.all });
  const create = useMutation({
    mutationFn: async (request: Parameters<typeof VaultAPI.createConversationSpace>[0]) => value(await VaultAPI.createConversationSpace(request)),
    onSuccess: invalidate,
  });
  const update = useMutation({
    mutationFn: async (request: Parameters<typeof VaultAPI.updateConversationSpace>[0]) => value(await VaultAPI.updateConversationSpace(request)),
    onSuccess: invalidate,
  });
  const archive = useMutation({
    mutationFn: async (request: Parameters<typeof VaultAPI.archiveConversationSpace>[0]) => value(await VaultAPI.archiveConversationSpace(request)),
    onSuccess: invalidate,
  });
  return { create, update, archive };
}
export function useSidebarBookmarksQuery(query: string, spaceId: string | null, enabled = true) {
  return useQuery({
    queryKey: [...SIDEBAR_BOOKMARKS_QUERY_KEY, query, spaceId],
    enabled,
    queryFn: async () => {
      const response = value(await VaultAPI.listMessageBookmarks({ query: query.trim() || undefined, limit: 60, offset: 0 }));
      const bookmarks = Array.isArray(response) ? [] : response.bookmarks;
      return spaceId ? bookmarks.filter(bookmark => bookmark.spaceId === spaceId) : bookmarks;
    },
  });
}
export function useSynthesizeConversationMutation() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, title }: { id: string; title: string }) => {
      const result = value(await VaultAPI.synthesizeJournalEntries({ conversationIds: [id], scope: 'conversation', maxEntries: 1 }));
      return value(await VaultAPI.quickCapture(buildSynthesisBlock({
        heading: title,
        entryCount: result.entryCount,
        synthesis: result.synthesis,
        citations: result.citations,
      }), result.sources ?? [], result.conversationIds));
    },
    onSuccess: () => client.invalidateQueries({ queryKey: ['workspace-notes'] }),
  });
}
