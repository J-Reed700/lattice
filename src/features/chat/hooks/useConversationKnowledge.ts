import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { VaultAPI, type ApiClient } from '@/lib/api';
import type { KnowledgeRequestDto } from '@/lib/bindings';

const listRequest = (conversationId: string, offset: number): KnowledgeRequestDto => ({
  conversationId, action: 'list', itemId: null, text: null, scope: null, kind: null,
  validFrom: null, validUntil: null, offset,
});

async function manageKnowledge(request: KnowledgeRequestDto, api: ApiClient = VaultAPI) {
  const result = await api.manageKnowledge(request);
  if (!result.ok) throw new Error(result.error);
  return result.data;
}

/** One page of the knowledge a conversation can reuse, polled while shown. */
export function useConversationKnowledgeQuery(conversationId: string, offset: number) {
  const api = useApi();
  return useQuery({
    queryKey: ['conversationKnowledge', conversationId, offset],
    queryFn: () => manageKnowledge(listRequest(conversationId, offset), api),
    refetchInterval: 5000,
  });
}

/**
 * Remembers, corrects, shares or forgets a knowledge item. `onSaved` runs
 * before the knowledge, memory and conversation caches refresh.
 */
export function useManageKnowledgeMutation(onSaved: (request: KnowledgeRequestDto) => void) {
  const api = useApi();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (arg0: Parameters<typeof manageKnowledge>[0]) => manageKnowledge(arg0, api),
    onSuccess: async (_data, request) => {
      onSaved(request);
      await queryClient.invalidateQueries({ queryKey: ['conversationKnowledge'] });
      await queryClient.invalidateQueries({ queryKey: ['conversationMemory'] });
      await queryClient.invalidateQueries({ queryKey: ['conversations'] });
    },
  });
}
