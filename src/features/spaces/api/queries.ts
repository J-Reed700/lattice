import { queryOptions, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import VaultAPI, { type ApiClient } from '@/lib/api';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import type { ConversationSpaceDto } from '@/types';
import { unwrapApiResult } from '@/types/api/result';

export const spaceKeys = {
  all: ['spaces'] as const,
  list: ['spaces', 'list'] as const,
};

export const fetchSpaces = async (api: ApiClient = VaultAPI): Promise<ConversationSpaceDto[]> =>
  unwrapApiResult(await api.listConversationSpaces());

export const spacesQueryOptions = (api: ApiClient = VaultAPI) =>
  queryOptions({
    queryKey: spaceKeys.list,
    queryFn: () => fetchSpaces(api),
    staleTime: 30_000,
  });

/** Every space, archived ones included, in the backend's order. */
export function useSpacesQuery() {
  const api = useApi();
  return useQuery(spacesQueryOptions(api));
}

type CreateSpaceRequest = Parameters<typeof VaultAPI.createConversationSpace>[0];
type UpdateSpaceRequest = Parameters<typeof VaultAPI.updateConversationSpace>[0];
type ArchiveSpaceRequest = Parameters<typeof VaultAPI.archiveConversationSpace>[0];

/**
 * Creating, editing and archiving a space. Each refreshes the spaces list;
 * archiving also refreshes conversation lists, which leave out an archived
 * space's chats.
 */
export function useSpaceMutations() {
  const api = useApi();
  const client = useQueryClient();
  const create = useMutation({
    mutationFn: async (request: CreateSpaceRequest) =>
      unwrapApiResult(await api.createConversationSpace(request)),
    onSuccess: () => client.invalidateQueries({ queryKey: spaceKeys.all }),
  });
  const update = useMutation({
    mutationFn: async (request: UpdateSpaceRequest) =>
      unwrapApiResult(await api.updateConversationSpace(request)),
    onSuccess: () => client.invalidateQueries({ queryKey: spaceKeys.all }),
  });
  const archive = useMutation({
    mutationFn: async (request: ArchiveSpaceRequest) =>
      unwrapApiResult(await api.archiveConversationSpace(request)),
    onSuccess: () => Promise.all([
      client.invalidateQueries({ queryKey: spaceKeys.all }),
      client.invalidateQueries({ queryKey: conversationKeys.all }),
    ]),
  });
  return { create, update, archive };
}
