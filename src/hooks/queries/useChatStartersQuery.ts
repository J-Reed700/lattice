import { useQuery } from '@tanstack/react-query';

import { VaultAPI } from '@/lib/api';
import type { ChatStarters } from '@/types/api/chatStarters';

export const CHAT_STARTERS_QUERY_KEY = ['chat', 'starters'] as const;

/** The key a single space's starters are cached under. */
export const chatStartersQueryKey = (spaceId: string | null) =>
  [...CHAT_STARTERS_QUERY_KEY, spaceId] as const;

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
 *
 * The space is part of the key: a chat may only ever see the documents in its
 * own space, so switching space must refetch rather than reuse questions drawn
 * from a library this chat cannot read.
 */
export function useChatStartersQuery(spaceId: string | null = null) {
  return useQuery<ChatStarters>({
    queryKey: chatStartersQueryKey(spaceId),
    queryFn: async () => {
      const result = await VaultAPI.generateChatStarters(spaceId);
      if (!result.ok) return EMPTY;
      return result.data;
    },
    staleTime: 30 * 60 * 1000,
    gcTime: 60 * 60 * 1000,
    retry: false,
  });
}
