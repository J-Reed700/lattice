import { useQuery } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';



/**
 * What a conversation's memory has recorded, read while the reader is open.
 * It re-reads every few seconds: a turn can record something while it is open.
 */
export function useConversationMemoryQuery(conversationId: string | null | undefined, includeHistory: boolean, isOpen: boolean) {
  const api = useApi();
  return useQuery({
    queryKey: ['conversationMemory', conversationId, includeHistory],
    enabled: isOpen && !!conversationId,
    refetchInterval: isOpen ? 5000 : false,
    retry: false,
    queryFn: async () => {
      const result = await api.getConversationMemory(conversationId!, includeHistory);
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
  });
}
