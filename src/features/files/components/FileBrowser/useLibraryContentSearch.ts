import { useEffect, useMemo, useRef } from 'react';

import { useApi } from '@/contexts/ApiContext';
import { readDocumentText } from '@/features/files/hooks/useLibraryDocumentActions';
import { useFileBrowserStore } from '@/features/files/stores/fileBrowserStore';
import { type DocumentMetadata } from '@/types/fileBrowser';

import { ContentSearchCache } from './contentSearchCache';
import { canSearchByContent, matchesTypeFilter, normalizeSearchToken } from './fileBrowserHelpers';

/**
 * Searches the text of the documents in the active type facet for the search
 * query, debounced and in small batches, and writes the matches to the store.
 */
export function useLibraryContentSearch(
  documents: DocumentMetadata[],
  searchQuery: string,
  filterByType: string | null
) {
  const api = useApi();
  const setContentSearchMatches = useFileBrowserStore(state => state.setContentSearchMatches);
  const setContentSearchLoading = useFileBrowserStore(state => state.setContentSearchLoading);
  const clearContentSearch = useFileBrowserStore(state => state.clearContentSearch);

  const contentCacheRef = useRef(new ContentSearchCache());
  const contentSearchSequenceRef = useRef(0);

  useEffect(() => {
    const currentIds = new Set(documents.map(doc => doc.id));

    contentCacheRef.current.retain(currentIds);
  }, [documents]);

  const normalizedSearchQuery = useMemo(() => normalizeSearchToken(searchQuery), [searchQuery]);
  const activeFilter = useMemo(() => normalizeSearchToken(filterByType ?? '') || null, [filterByType]);

  const contentSearchScope = useMemo(
    () => documents.filter(doc => matchesTypeFilter(doc, activeFilter)),
    [activeFilter, documents]
  );

  useEffect(() => {
    const requestId = contentSearchSequenceRef.current + 1;
    contentSearchSequenceRef.current = requestId;

    if (!normalizedSearchQuery) {
      clearContentSearch();
      return;
    }

    const documentsToCheck = contentSearchScope.filter((doc) => {
      const fileNameMatches = normalizeSearchToken(doc.fileName).includes(normalizedSearchQuery);
      return !fileNameMatches && canSearchByContent(doc);
    });

    if (documentsToCheck.length === 0) {
      setContentSearchMatches([]);
      setContentSearchLoading(false);
      return;
    }

    let cancelled = false;

    const timeoutId = window.setTimeout(() => {
      void (async () => {
        setContentSearchLoading(true);

        const contentMatches = new Set<string>();
        const batchSize = 6;

        for (let start = 0; start < documentsToCheck.length; start += batchSize) {
          if (cancelled || requestId !== contentSearchSequenceRef.current) {
            return;
          }

          const batch = documentsToCheck.slice(start, start + batchSize);
          const batchResults = await Promise.all(batch.map(async (doc) => {
            const version = JSON.stringify([doc.filePath, doc.modifiedAt, doc.indexedAt]);
            const cachedContent = contentCacheRef.current.get(doc.id, version);
            if (typeof cachedContent === 'string') {
              return cachedContent.includes(normalizedSearchQuery) ? doc.id : null;
            }

            const result = await readDocumentText(doc.filePath, api);
            if (!result.ok) {
              return null;
            }

            const normalizedContent = result.data.toLowerCase();
            contentCacheRef.current.set(doc.id, version, normalizedContent);

            return normalizedContent.includes(normalizedSearchQuery) ? doc.id : null;
          }));

          for (const docId of batchResults) {
            if (docId) {
              contentMatches.add(docId);
            }
          }
        }

        if (!cancelled && requestId === contentSearchSequenceRef.current) {
          setContentSearchMatches(contentMatches);
          setContentSearchLoading(false);
        }
      })().catch(() => {
        if (!cancelled && requestId === contentSearchSequenceRef.current) {
          setContentSearchMatches([]);
          setContentSearchLoading(false);
        }
      });
    }, 220);

    return () => {
      cancelled = true;
      window.clearTimeout(timeoutId);
    };
  }, [api, clearContentSearch, contentSearchScope, normalizedSearchQuery, setContentSearchLoading, setContentSearchMatches]);

  return { activeFilter };
}
