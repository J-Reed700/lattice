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
import type { ApiResult } from '@/types';
import type { ConversationJournalDto } from '@/types/api/conversation';
import { unwrapApiResult } from '@/types/api/result';
import { buildCapturedChatReferenceIndex } from '@/utils/chatReferenceIndex';

export const JOURNALS_QUERY_KEY = ['journals'] as const;
export const WORKSPACE_NOTES_QUERY_KEY = ['workspace-notes'] as const;
export const journalsQueryOptions = (api: ApiClient = VaultAPI) =>
  queryOptions({
    queryKey: JOURNALS_QUERY_KEY,
    queryFn: async () => unwrapApiResult(await api.listJournals()),
    retry: 1,
  });
const EMPTY_JOURNALS: ConversationJournalDto[] = [];
export function useJournalsQuery() {
  const api = useApi();
  const query = useQuery(journalsQueryOptions(api));
  return { ...query, journals: query.data ?? EMPTY_JOURNALS };
}
export const workspaceNotesQueryOptions = (journalId?: string, api: ApiClient = VaultAPI) =>
  queryOptions({
    queryKey: [...WORKSPACE_NOTES_QUERY_KEY, 'list', journalId ?? null],
    queryFn: async () =>
      unwrapApiResult(await api.listWorkspaceNotes(journalId)).notes,
  });
export function useWorkspaceNotesQuery(journalId?: string) {
  const api = useApi();
  return useQuery(workspaceNotesQueryOptions(journalId, api));
}
export function useCapturedReferencesQuery(enabled = true) {
  const api = useApi();
  return useQuery({
    ...workspaceNotesQueryOptions(undefined, api),
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
  api: ApiClient = VaultAPI,
) => persistJournal(client, api.createJournal(request));
export const updateJournal = (
  client: QueryClient,
  request: Parameters<typeof VaultAPI.updateJournal>[0],
  api: ApiClient = VaultAPI,
) => persistJournal(client, api.updateJournal(request));
export const deleteJournal = (
  client: QueryClient,
  request: Parameters<typeof VaultAPI.deleteJournal>[0],
  api: ApiClient = VaultAPI,
) => persistJournal(client, api.deleteJournal(request));

/** Under the journals prefix, so a journal's create or delete refreshes its pins. */
export const journalEntryPinsKey = (journalSpaceId: string) =>
  [...JOURNALS_QUERY_KEY, 'entry-pins', journalSpaceId] as const;
export const journalEntryPinsQueryOptions = (journalSpaceId: string, api: ApiClient = VaultAPI) =>
  queryOptions({
    queryKey: journalEntryPinsKey(journalSpaceId),
    queryFn: async () => unwrapApiResult(await api.listJournalEntryPins(journalSpaceId)),
  });

/**
 * Pins or unpins an entry in one journal. The pin shows at once and is put
 * back if the backend refuses it.
 */
export function useSetJournalEntryPinnedMutation() {
  const api = useApi();
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (request: Parameters<typeof VaultAPI.setJournalEntryPinned>[0]) =>
      unwrapApiResult(await api.setJournalEntryPinned(request)),
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

/**
 * The space a saved conversation searches; null for General or a row that
 * cannot be read. Reads the saved row, not the sidebar's list, which may be
 * showing another space.
 */
export async function fetchConversationSpaceId(conversationId: string, api: ApiClient = VaultAPI): Promise<string | null> {
  const result = await api.getConversation(conversationId);
  return result.ok ? (result.data.conversation?.spaceId ?? null) : null;
}
