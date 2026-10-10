import { useQuery } from '@tanstack/react-query';

import { VaultAPI } from '@/lib/api';

/**
 * What a conversation's memory has recorded, read while the reader is open.
 * It re-reads every few seconds: a turn can record something while it is open.
 */
export function useConversationMemoryQuery(conversationId: string | null | undefined, includeHistory: boolean, isOpen: boolean) {
  return useQuery({
    queryKey: ['conversationMemory', conversationId, includeHistory],
    enabled: isOpen && !!conversationId,
    refetchInterval: isOpen ? 5000 : false,
    retry: false,
    queryFn: async () => {
      const result = await VaultAPI.getConversationMemory(conversationId!, includeHistory);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
  });
}
