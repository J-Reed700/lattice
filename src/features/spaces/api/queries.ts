import { queryOptions, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { conversationKeys } from '@/hooks/queries/conversationKeys';
import VaultAPI from '@/lib/api';
import type { ConversationSpaceDto } from '@/types';
import { unwrapApiResult } from '@/types/api/result';

export const spaceKeys = {
  all: ['spaces'] as const,
  list: ['spaces', 'list'] as const,
};

export const fetchSpaces = async (): Promise<ConversationSpaceDto[]> =>
  unwrapApiResult(await VaultAPI.listConversationSpaces());

export const spacesQueryOptions = () =>
  queryOptions({
    queryKey: spaceKeys.list,
    queryFn: fetchSpaces,
    staleTime: 30_000,
  });

/** Every space, archived ones included, in the backend's order. */
export function useSpacesQuery() {
  return useQuery(spacesQueryOptions());
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
  const client = useQueryClient();
  const create = useMutation({
    mutationFn: async (request: CreateSpaceRequest) =>
      unwrapApiResult(await VaultAPI.createConversationSpace(request)),
    onSuccess: () => client.invalidateQueries({ queryKey: spaceKeys.all }),
  });
  const update = useMutation({
    mutationFn: async (request: UpdateSpaceRequest) =>
      unwrapApiResult(await VaultAPI.updateConversationSpace(request)),
    onSuccess: () => client.invalidateQueries({ queryKey: spaceKeys.all }),
  });
  const archive = useMutation({
    mutationFn: async (request: ArchiveSpaceRequest) =>
      unwrapApiResult(await VaultAPI.archiveConversationSpace(request)),
    onSuccess: () => Promise.all([
      client.invalidateQueries({ queryKey: spaceKeys.all }),
      client.invalidateQueries({ queryKey: conversationKeys.all }),
    ]),
  });
  return { create, update, archive };
}
