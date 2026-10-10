import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from '@tanstack/react-query';

import { VaultAPI } from '@/lib/api';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import type { ApiResult } from '@/types';
import type { ConversationJournalDto } from '@/types/api/conversation';
import { unwrapApiResult } from '@/types/api/result';
import { buildCapturedChatReferenceIndex } from '@/utils/chatReferenceIndex';

export const JOURNALS_QUERY_KEY = ['journals'] as const;
export const WORKSPACE_NOTES_QUERY_KEY = ['workspace-notes'] as const;
export const journalsQueryOptions = () =>
  queryOptions({
    queryKey: JOURNALS_QUERY_KEY,
    queryFn: async () => unwrapApiResult(await VaultAPI.listJournals()),
    retry: 1,
  });
const EMPTY_JOURNALS: ConversationJournalDto[] = [];
export function useJournalsQuery() {
  const query = useQuery(journalsQueryOptions());
  return { ...query, journals: query.data ?? EMPTY_JOURNALS };
}
export const workspaceNotesQueryOptions = (journalId?: string) =>
  queryOptions({
    queryKey: [...WORKSPACE_NOTES_QUERY_KEY, 'list', journalId ?? null],
    queryFn: async () =>
      unwrapApiResult(await VaultAPI.listWorkspaceNotes(journalId)).notes,
  });
export function useWorkspaceNotesQuery(journalId?: string) {
  return useQuery(workspaceNotesQueryOptions(journalId));
}
export function useCapturedReferencesQuery(enabled = true) {
  return useQuery({
    ...workspaceNotesQueryOptions(),
    enabled,
    select: buildCapturedChatReferenceIndex,
  });
}
async function persistJournal<T>(
  client: QueryClient,
  operation: Promise<ApiResult<T>>,
) {
  const result = await operation;
  if (result.ok) {
    await Promise.all([
      client.invalidateQueries({ queryKey: JOURNALS_QUERY_KEY }),
      client.invalidateQueries({ queryKey: WORKSPACE_NOTES_QUERY_KEY }),
      client.invalidateQueries({ queryKey: conversationKeys.all }),
    ]);
  }
  return result;
}
export const createJournal = (
  client: QueryClient,
  request: Parameters<typeof VaultAPI.createJournal>[0],
) => persistJournal(client, VaultAPI.createJournal(request));
export const updateJournal = (
  client: QueryClient,
  request: Parameters<typeof VaultAPI.updateJournal>[0],
) => persistJournal(client, VaultAPI.updateJournal(request));
export const deleteJournal = (
  client: QueryClient,
  request: Parameters<typeof VaultAPI.deleteJournal>[0],
) => persistJournal(client, VaultAPI.deleteJournal(request));

/** Under the journals prefix, so a journal's create or delete refreshes its pins. */
export const journalEntryPinsKey = (journalSpaceId: string) =>
  [...JOURNALS_QUERY_KEY, 'entry-pins', journalSpaceId] as const;
export const journalEntryPinsQueryOptions = (journalSpaceId: string) =>
  queryOptions({
    queryKey: journalEntryPinsKey(journalSpaceId),
    queryFn: async () => unwrapApiResult(await VaultAPI.listJournalEntryPins(journalSpaceId)),
  });

/**
 * Pins or unpins an entry in one journal. The pin shows at once and is put
 * back if the backend refuses it.
 */
export function useSetJournalEntryPinnedMutation() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (request: Parameters<typeof VaultAPI.setJournalEntryPinned>[0]) =>
      unwrapApiResult(await VaultAPI.setJournalEntryPinned(request)),
    onMutate: async ({ journalSpaceId, conversationId, pinned }) => {
      const key = journalEntryPinsKey(journalSpaceId);
      await client.cancelQueries({ queryKey: key });
      const previous = client.getQueryData<string[]>(key);
      client.setQueryData<string[]>(key, (current = []) => {
        const rest = current.filter((id) => id !== conversationId);
        return pinned ? [conversationId, ...rest] : rest;
      });
      return { previous };
    },
    onError: (_error, { journalSpaceId }, context) => {
      client.setQueryData(journalEntryPinsKey(journalSpaceId), context?.previous);
    },
    onSettled: (_data, _error, { journalSpaceId }) =>
      client.invalidateQueries({ queryKey: journalEntryPinsKey(journalSpaceId) }),
  });
}
