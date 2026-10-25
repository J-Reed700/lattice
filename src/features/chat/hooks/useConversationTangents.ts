import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { unwrap } from '@/features/chat/api/conversationQueryData';
import type { ConversationTangentDto, CreateTangentRequestDto } from '@/lib/bindings';
import { conversationKeys } from '@/shared/conversations/conversationKeys';

/** The tangents opened from one conversation, newest first. */
export function useConversationTangentsQuery(conversationId: string) {
  const api = useApi();
  return useQuery({
    queryKey: conversationKeys.tangents(conversationId),
    queryFn: async () => unwrap(await api.listConversationTangents(conversationId)),
    staleTime: 10_000,
  });
}

/** Starts a tangent from a selection and puts it first in its parent's list. */
export function useCreateTangentMutation(conversationId: string) {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (selection: CreateTangentRequestDto) => unwrap(await api.createConversationTangent(selection)),
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
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (tangent: ConversationTangentDto) => unwrap(await api.promoteConversationTangent(tangent.conversationId)),
    onSuccess: async (conversation, tangent) => {
      queryClient.setQueryData(conversationKeys.detail(tangent.conversationId), conversation);
      await queryClient.invalidateQueries({ queryKey: conversationKeys.lists });
    },
  });
}
