import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { unwrap } from '@/features/chat/api/conversationQueryData';
import { VaultAPI } from '@/lib/api';
import type { ConversationTangentDto, CreateTangentRequestDto } from '@/lib/bindings';
import { conversationKeys } from '@/shared/conversations/conversationKeys';

/** The tangents opened from one conversation, newest first. */
export function useConversationTangentsQuery(conversationId: string) {
  return useQuery({
    queryKey: conversationKeys.tangents(conversationId),
    queryFn: async () => unwrap(await VaultAPI.listConversationTangents(conversationId)),
    staleTime: 10_000,
  });
}

/** Starts a tangent from a selection and puts it first in its parent's list. */
export function useCreateTangentMutation(conversationId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (selection: CreateTangentRequestDto) => unwrap(await VaultAPI.createConversationTangent(selection)),
    onSuccess: (tangent) => {
      queryClient.setQueryData<ConversationTangentDto[]>(conversationKeys.tangents(conversationId), current =>
        [tangent, ...(current ?? []).filter(item => item.conversationId !== tangent.conversationId)]);
    },
  });
}

/**
 * Makes a tangent a conversation of its own: its detail is seeded, the lists
 * refresh, and it leaves its parent's tangents.
 */
export function usePromoteTangentMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (tangent: ConversationTangentDto) => unwrap(await VaultAPI.promoteConversationTangent(tangent.conversationId)),
    onSuccess: async (conversation, tangent) => {
      queryClient.setQueryData(conversationKeys.detail(tangent.conversationId), conversation);
      await queryClient.invalidateQueries({ queryKey: conversationKeys.lists });
    },
  });
}
