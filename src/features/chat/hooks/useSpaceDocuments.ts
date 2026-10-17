import { useQuery } from '@tanstack/react-query';

import { spaceKeys } from '@/features/spaces/api/queries';
import { useDebounce } from '@/hooks/useDebounce';
import { VaultAPI } from '@/lib/api';
import type { SpaceDocument } from '@/types';

/**
 * The documents `@` may offer this conversation.
 *
 * `list_space_documents` answers from the same allow-list retrieval uses, so
 * what the popup shows is exactly what a turn can read: the chat's space, plus
 * the files attached to this chat, which belong to no space. It is the only
 * source here on purpose: a general document list would offer files this chat
 * cannot open, and space isolation fails closed rather than apologising
 * afterwards.
 */

/** Eight rows is a menu; more is a file browser, and there is one of those. */
const MENTION_LIMIT = 8;
/** Long enough that typing a file name is one search, not eight. */
const DEBOUNCE_MS = 120;

export interface SpaceDocumentsResult {
  documents: SpaceDocument[];
  isLoading: boolean;
}

const NO_DOCUMENTS: SpaceDocument[] = [];

const spaceDocumentsKey = (spaceId: string | null, conversationId: string | null, query: string) =>
  [...spaceKeys.all, 'documents', spaceId, conversationId, query] as const;

/** `query` of null means nothing is being looked up. */
export function useSpaceDocuments(
  spaceId: string | null,
  conversationId: string | null,
  query: string | null
): SpaceDocumentsResult {
  const debouncedQuery = useDebounce(query, DEBOUNCE_MS);
  const lookup = query === null ? null : debouncedQuery;

  const documentsQuery = useQuery({
    queryKey: spaceDocumentsKey(spaceId, conversationId, lookup ?? ''),
    queryFn: async () => {
      const result = await VaultAPI.listSpaceDocuments(spaceId, conversationId, lookup ?? '', MENTION_LIMIT);
      // A failed lookup offers nothing rather than offering the last space's
      // answer: the chips it builds decide what the turn is allowed to read.
      return result.ok ? result.data : NO_DOCUMENTS;
    },
    enabled: lookup !== null,
    // While the next keystroke is looked up, the list it refines stays up, but
    // never a list from another space or conversation.
    placeholderData: (previous, previousQuery) =>
      previousQuery?.queryKey[2] === spaceId && previousQuery.queryKey[3] === conversationId ? previous : undefined,
    staleTime: 0,
  });

  if (query === null) return { documents: NO_DOCUMENTS, isLoading: false };
  return {
    documents: lookup === null ? NO_DOCUMENTS : documentsQuery.data ?? NO_DOCUMENTS,
    isLoading: query !== debouncedQuery || documentsQuery.isFetching,
  };
}
