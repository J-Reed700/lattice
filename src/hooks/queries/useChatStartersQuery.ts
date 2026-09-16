import { useQuery } from '@tanstack/react-query';

import { VaultAPI } from '@/lib/api';
import type { ChatStarters } from '@/types/api/chatStarters';

export const CHAT_STARTERS_QUERY_KEY = ['chat', 'starters'] as const;

const EMPTY: ChatStarters = {
  fingerprint: '',
  generatedAt: '',
  starters: [],
  documentCount: 0,
};

/**
 * Corpus-derived opening questions for the empty chat state.
 *
 * A failure degrades to an empty payload rather than an error state: the empty
 * state's job is to be quiet and honest, and "we couldn't generate questions"
 * is not something the reader needs to be told.
 */
export function useChatStartersQuery() {
  return useQuery<ChatStarters>({
    queryKey: CHAT_STARTERS_QUERY_KEY,
    queryFn: async () => {
      const result = await VaultAPI.generateChatStarters();
      if (!result.ok) return EMPTY;
      return result.data;
    },
    staleTime: 30 * 60 * 1000,
    gcTime: 60 * 60 * 1000,
    retry: false,
  });
}
