import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { VaultAPI } from '@/lib/api';
import type { KnowledgeRequestDto } from '@/lib/bindings';

const listRequest = (conversationId: string, offset: number): KnowledgeRequestDto => ({
  conversationId, action: 'list', itemId: null, text: null, scope: null, kind: null,
  validFrom: null, validUntil: null, offset,
});

async function manageKnowledge(request: KnowledgeRequestDto) {
  const result = await VaultAPI.manageKnowledge(request);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

/** One page of the knowledge a conversation can reuse, polled while shown. */
export function useConversationKnowledgeQuery(conversationId: string, offset: number) {
  return useQuery({
    queryKey: ['conversationKnowledge', conversationId, offset],
    queryFn: () => manageKnowledge(listRequest(conversationId, offset)),
    refetchInterval: 5000,
  });
}

/**
 * Remembers, corrects, shares or forgets a knowledge item. `onSaved` runs
 * before the knowledge, memory and conversation caches refresh.
 */
export function useManageKnowledgeMutation(onSaved: (request: KnowledgeRequestDto) => void) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: manageKnowledge,
    onSuccess: async (_data, request) => {
      onSaved(request);
      await queryClient.invalidateQueries({ queryKey: ['conversationKnowledge'] });
      await queryClient.invalidateQueries({ queryKey: ['conversationMemory'] });
      await queryClient.invalidateQueries({ queryKey: ['conversations'] });
    },
  });
}
