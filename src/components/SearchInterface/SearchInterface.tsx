import { useState, useCallback, useMemo } from 'react';

import { PageHeader, SidebarTabs } from '@/components/ui';
import { useSearchQuery } from '@/hooks/queries';
import { useDebounce } from '@/hooks/useDebounce';
import VaultAPI from '@/lib/api';
import type { SearchResult } from '@/types';

import { SearchEmptyState } from './SearchEmptyState';
import { SearchInput } from './SearchInput';
import { ContentViewer } from '../ContentViewer';
import { VirtualizedSearchResults } from '../VirtualList/VirtualList';

type SearchMode = 'hybrid' | 'semantic' | 'keyword';

const SEARCH_MODES: ReadonlyArray<{ id: SearchMode; label: string }> = [
  { id: 'hybrid', label: 'Hybrid' },
  { id: 'semantic', label: 'Semantic' },
  { id: 'keyword', label: 'Keyword' },
];

/**
 * SearchInterface
 *
 * The "I know the file exists, find it" escape hatch. Reading-column
 * anatomy A: one PageHeader, one input, one row of mode tabs, then
 * hairline result rows.
 *
 * Data: TanStack Query (useSearchQuery, 300ms debounce).
 */

export function SearchInterface() {
  const [query, setQuery] = useState('');
  const [searchMode, setSearchMode] = useState<SearchMode>('hybrid');
  const [viewerFilePath, setViewerFilePath] = useState<string | null>(null);
  const [openError, setOpenError] = useState<string | null>(null);

  // Debounce query to avoid excessive API calls
  const debouncedQuery = useDebounce(query, 300);

  const {
    data: results = [],
    isLoading: isSearching,
    error,
  } = useSearchQuery({
    query: debouncedQuery,
    mode: searchMode,
    limit: 20,
  });

  const hasSearched = debouncedQuery.trim().length > 0;

  const groupedResults = useMemo<SearchResult[]>(() => {
    if (results.length === 0) {
      return [];
    }

    const mergeMaxScore = (
      current: number | null | undefined,
      incoming: number | null | undefined
    ): number | null => {
      if (current == null && incoming == null) {
        return null;
      }
      return Math.max(current ?? Number.NEGATIVE_INFINITY, incoming ?? Number.NEGATIVE_INFINITY);
    };

    const mergeMinRank = (
      current: number | null | undefined,
      incoming: number | null | undefined
    ): number | null => {
      if (current == null && incoming == null) {
        return null;
      }
      return Math.min(current ?? Number.MAX_SAFE_INTEGER, incoming ?? Number.MAX_SAFE_INTEGER);
    };

    const grouped = new Map<string, SearchResult>();

    for (const rawResult of results) {
      const key = rawResult.documentId || rawResult.id;
      const excerpt = rawResult.content?.trim();
      const existing = grouped.get(key);

      if (!existing) {
        grouped.set(key, {
          ...rawResult,
          id: key,
          documentId: rawResult.documentId || key,
          highlights: excerpt ? [excerpt] : [],
        });
        continue;
      }

      const isHigherRanked = rawResult.score > existing.score;
      if (isHigherRanked) {
        existing.score = rawResult.score;
        existing.content = rawResult.content;
        existing.path = rawResult.path ?? existing.path;
        existing.title = rawResult.title || existing.title;
      } else {
        existing.path = existing.path ?? rawResult.path;
        existing.title = existing.title || rawResult.title;
      }

      existing.vectorScore = mergeMaxScore(existing.vectorScore, rawResult.vectorScore);
      existing.bm25Score = mergeMaxScore(existing.bm25Score, rawResult.bm25Score);
      existing.vectorRank = mergeMinRank(existing.vectorRank, rawResult.vectorRank);
      existing.bm25Rank = mergeMinRank(existing.bm25Rank, rawResult.bm25Rank);
      existing.metadata = Object.keys(existing.metadata || {}).length > 0
        ? existing.metadata
        : rawResult.metadata;

      if (excerpt) {
        const currentHighlights = existing.highlights || [];
        if (!currentHighlights.includes(excerpt)) {
          existing.highlights = [...currentHighlights, excerpt];
        }
      }
    }

    const deduplicated = Array.from(grouped.values())
      .map((result) => ({
        ...result,
        highlights: (result.highlights || []).slice(0, 3),
      }))
      .sort((a, b) => b.score - a.score);

    const maxScore = deduplicated[0]?.score ?? 0;
    if (maxScore > 0 && maxScore < 0.1) {
      return deduplicated.map((result) => ({
        ...result,
        score: Math.min(1, result.score / maxScore),
      }));
    }

    return deduplicated;
  }, [results]);

  const handleQueryChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      setQuery(e.target.value);
    },
    []
  );

  const handleResultOpen = useCallback(async (result: SearchResult) => {
    setOpenError(null);

    const targetId = result.documentId || result.id;
    if (targetId) {
      const byId = await VaultAPI.openFileById(targetId);
      if (byId.ok) {
        if (byId.data.action === 'render_internal') {
          setViewerFilePath(byId.data.contentPath);
        }
        return;
      }

      if (typeof result.path === 'string' && result.path.length > 0) {
        const byPath = await VaultAPI.openFile(result.path);
        if (byPath.ok) {
          if (byPath.data.action === 'render_internal') {
            setViewerFilePath(byPath.data.contentPath);
          }
          return;
        }

        setOpenError(byPath.error || byId.error || "Couldn't open result");
        return;
      }

      setOpenError(byId.error || "Couldn't open result");
      return;
    }

    setOpenError('Result has no document identifier');
  }, []);

  return (
    <main className="h-full overflow-y-auto bg-bg">
      <div className="mx-auto w-full max-w-[760px] px-6 pt-10 pb-16">
        <PageHeader
          title="Search"
          meta={
            groupedResults.length > 0
              ? `${groupedResults.length} ${groupedResults.length === 1 ? 'result' : 'results'}`
              : undefined
          }
        />

        <SearchInput value={query} onChange={handleQueryChange} isSearching={isSearching} />
        <SidebarTabs
          value={searchMode}
          onChange={setSearchMode}
          options={SEARCH_MODES}
          className="mt-3 text-sm"
        />

        {error && (
          <p className="mt-4 text-sm text-danger-fg">Couldn&apos;t search. {error.message}</p>
        )}
        {openError && <p className="mt-4 text-sm text-danger-fg">{openError}</p>}

        <div className="mt-6">
          {groupedResults.length === 0 ? (
            <SearchEmptyState
              hasSearched={hasSearched}
              query={debouncedQuery}
              isSearching={isSearching}
              onPickRecent={setQuery}
            />
          ) : (
            <VirtualizedSearchResults
              results={groupedResults}
              query={debouncedQuery}
              onResultOpen={handleResultOpen}
            />
          )}
        </div>
      </div>

      <ContentViewer filePath={viewerFilePath} onClose={() => setViewerFilePath(null)} />
    </main>
  );
}

SearchInterface.displayName = 'SearchInterface';
