import { useEffect, useRef, useState } from 'react';

import { useDebounce } from '../../hooks/useDebounce';
import VaultAPI from '../../lib/api';
import { logger } from '../../utils/logger';

import type {
  ConversationDto,
  ConversationJournalDto,
  ConversationMessageBookmarkDto,
  DocumentMetadata,
  SearchResult,
} from '../../types';

export interface SpotlightFolder {
  path: string;
  documentCount: number;
}

export interface SpotlightResults {
  documents: SearchResult[];
  conversations: ConversationDto[];
  bookmarks: ConversationMessageBookmarkDto[];
  journals: ConversationJournalDto[];
  folders: SpotlightFolder[];
  isLoading: boolean;
}

const DEBOUNCE_MS = 180;

/**
 * Derive unique folder paths from a flat list of documents.
 *
 * Folder search is entirely client-side today — the API has no
 * `listFolders` endpoint, and `listAllDocuments` is cheap.
 */
function deriveFolders(
  documents: DocumentMetadata[],
  query: string,
  limit: number
): SpotlightFolder[] {
  const counts = new Map<string, number>();
  for (const doc of documents) {
    const path = doc.filePath ?? '';
    const lastSlash = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
    if (lastSlash <= 0) continue;
    const folder = path.slice(0, lastSlash);
    counts.set(folder, (counts.get(folder) ?? 0) + 1);
  }

  const q = query.trim().toLowerCase();
  const folders: SpotlightFolder[] = [];
  for (const [path, documentCount] of counts) {
    if (q && !path.toLowerCase().includes(q)) continue;
    folders.push({ path, documentCount });
  }

  folders.sort((a, b) => b.documentCount - a.documentCount);
  return folders.slice(0, limit);
}

/**
 * Client-side title fuzzy filter for journals.
 *
 * TODO(backend): real journal body search requires a
 * `searchJournalEntries` endpoint. `listJournals` currently returns
 * conversation-linked journals only, so a user with journal prose that
 * never got linked to a conversation won't surface here. Flag for the
 * backend phase — track alongside Phase 5.1.1.
 */
function filterJournals(
  journals: ConversationJournalDto[],
  query: string,
  limit: number
): ConversationJournalDto[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const matches = journals.filter((journal) => {
    if (journal.name.toLowerCase().includes(q)) return true;
    if (journal.description?.toLowerCase().includes(q)) return true;
    return false;
  });
  return matches.slice(0, limit);
}

/**
 * useSpotlightResults
 *
 * Debounces the query, fans out to the corpus APIs in parallel, and
 * collates results for the unified Spotlight. Each group has its own
 * bounded limit; empty groups simply produce empty arrays so the
 * Spotlight can hide them at render time.
 *
 * Cancellation: every new debounced query cancels the previous
 * Promise.all via a `cancelled` ref so we never commit stale results.
 *
 * Corpus primitives fanned out:
 *   - Documents  → hybrid searchDocuments
 *   - Conversations → listConversationsExplorer
 *   - Bookmarks  → listMessageBookmarks
 *   - Journals   → listJournals + client title filter (see TODO above)
 *   - Folders    → derived from listAllDocuments, client filter
 *
 * When the palette is closed or the query is empty we skip the
 * corpus calls entirely — the empty-state view only needs recents
 * and static actions.
 */
export function useSpotlightResults(
  query: string,
  enabled: boolean
): SpotlightResults {
  const debouncedQuery = useDebounce(query, DEBOUNCE_MS);
  const [results, setResults] = useState<SpotlightResults>({
    documents: [],
    conversations: [],
    bookmarks: [],
    journals: [],
    folders: [],
    isLoading: false,
  });
  const cancelledRef = useRef(false);

  useEffect(() => {
    cancelledRef.current = false;

    if (!enabled) {
      setResults({
        documents: [],
        conversations: [],
        bookmarks: [],
        journals: [],
        folders: [],
        isLoading: false,
      });
      return;
    }

    const q = debouncedQuery.trim();
    if (!q) {
      setResults({
        documents: [],
        conversations: [],
        bookmarks: [],
        journals: [],
        folders: [],
        isLoading: false,
      });
      return;
    }

    setResults((prev) => ({ ...prev, isLoading: true }));

    void (async () => {
      try {
        const [
          docsResult,
          convsResult,
          bookmarksResult,
          journalsResult,
          allDocsResult,
        ] = await Promise.all([
          q.length >= 2
            ? VaultAPI.searchDocuments({
                query: q,
                limit: 8,
                searchMode: 'hybrid',
              })
            : Promise.resolve({ ok: true as const, data: [] as SearchResult[] }),
          VaultAPI.listConversationsExplorer({
            query: q,
            includeArchived: true,
            limit: 8,
            offset: 0,
          }),
          VaultAPI.listMessageBookmarks({
            query: q,
            limit: 8,
            offset: 0,
          }),
          VaultAPI.listJournals(),
          VaultAPI.listAllDocuments(2000),
        ]);

        if (cancelledRef.current) return;

        const documents =
          docsResult.ok && Array.isArray(docsResult.data) ? docsResult.data : [];

        const conversations = convsResult.ok
          ? Array.isArray(convsResult.data)
            ? []
            : convsResult.data.conversations
          : [];

        const bookmarks = bookmarksResult.ok
          ? Array.isArray(bookmarksResult.data)
            ? []
            : bookmarksResult.data.bookmarks
          : [];

        const journals = journalsResult.ok
          ? filterJournals(journalsResult.data, q, 8)
          : [];

        const folders = allDocsResult.ok
          ? deriveFolders(allDocsResult.data, q, 6)
          : [];

        setResults({
          documents,
          conversations,
          bookmarks,
          journals,
          folders,
          isLoading: false,
        });
      } catch (error) {
        if (cancelledRef.current) return;
        logger.error('Spotlight fan-out failed', {
          component: 'spotlight',
          error: error instanceof Error ? error.message : String(error),
        });
        setResults({
          documents: [],
          conversations: [],
          bookmarks: [],
          journals: [],
          folders: [],
          isLoading: false,
        });
      }
    })();

    return () => {
      cancelledRef.current = true;
    };
  }, [debouncedQuery, enabled]);

  return results;
}
