import { useEffect, useState } from 'react';

import { VaultAPI } from '../../../lib/api';

import type { SpaceDocument } from '../../../types';

/**
 * The documents `@` may offer, for the space the conversation is in.
 *
 * `list_space_documents` answers from the same allow-list retrieval uses, so
 * what the popup shows is exactly what a turn can read. It is the only source
 * here on purpose: a general document list would offer files this chat cannot
 * open, and space isolation fails closed rather than apologising afterwards.
 */

/** Eight rows is a menu; more is a file browser, and there is one of those. */
const MENTION_LIMIT = 8;
/** Long enough that typing a file name is one search, not eight. */
const DEBOUNCE_MS = 120;

export interface SpaceDocumentsResult {
  documents: SpaceDocument[];
  isLoading: boolean;
}

/** `query` of null means nothing is being looked up. */
export function useSpaceDocuments(spaceId: string | null, query: string | null): SpaceDocumentsResult {
  const [documents, setDocuments] = useState<SpaceDocument[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  useEffect(() => {
    if (query === null) {
      setDocuments([]);
      setIsLoading(false);
      return;
    }

    let cancelled = false;
    setIsLoading(true);
    const timer = setTimeout(() => {
      void VaultAPI.listSpaceDocuments(spaceId, query, MENTION_LIMIT).then((result) => {
        if (cancelled) return;
        // A failed lookup offers nothing rather than offering the last space's
        // answer: the chips it builds decide what the turn is allowed to read.
        setDocuments(result.ok ? result.data : []);
        setIsLoading(false);
      });
    }, DEBOUNCE_MS);

    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [spaceId, query]);

  return { documents, isLoading };
}
