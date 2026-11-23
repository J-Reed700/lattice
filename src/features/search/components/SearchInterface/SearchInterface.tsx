import { useState, useCallback, useEffect, useMemo, useRef } from 'react';

import { X } from 'lucide-react';
import { useInRouterContext, useLocation, useNavigate, useSearchParams } from 'react-router';

import { Button, PageHeader, Tabs, TabsList, TabsTrigger, TooltipProvider } from '@/components/ui';
import { useApi } from '@/contexts/ApiContext';
import { openDocumentFile, openFilePath } from '@/features/files/hooks/useLibraryDocumentActions';
import { FilePreviewModal } from '@/features/reading/components/FilePreviewModal';
import { fromSearchResult, type ResolvedReaderTarget } from '@/features/reading/model/sourceAdapter';
import { useSearchQuery } from '@/features/search/hooks/useSearchQuery';
import { useDebounce } from '@/hooks/useDebounce';
import { cn } from '@/lib/utils';
import type { SearchResult } from '@/types';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

import { SearchEmptyState } from './SearchEmptyState';
import { extractSearchFacets, filterSearchResults } from './searchFacets';
import { SearchInput } from './SearchInput';
import { SearchPreviewPane } from './SearchPreviewPane';
import { VirtualizedSearchResults } from '../VirtualList/VirtualList';

type SearchMode = 'hybrid' | 'semantic' | 'keyword';

const SEARCH_MODES: ReadonlyArray<{ id: SearchMode; label: string }> = [
  { id: 'hybrid', label: 'Hybrid' },
  { id: 'semantic', label: 'Semantic' },
  { id: 'keyword', label: 'Keyword' },
];

const WIDE_WORKBENCH_MEDIA = '(min-width: 1024px)';

function useIsWideScreen(): boolean {
  const [isWide, setIsWide] = useState(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
      return false;
    }
    return window.matchMedia(WIDE_WORKBENCH_MEDIA).matches;
  });

  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
      return;
    }
    const mq = window.matchMedia(WIDE_WORKBENCH_MEDIA);
    setIsWide(mq.matches);
    const onChange = (event: MediaQueryListEvent) => setIsWide(event.matches);
    mq.addEventListener?.('change', onChange);
    return () => mq.removeEventListener?.('change', onChange);
  }, []);

  return isWide;
}

/**
 * SearchInterface
 *
 * Minimalist 2-pane discovery workbench: crisp PageHeader, search input,
 * unified pill mode Tabs, live metadata filter facets, clean result rows with
 * relevance meter (raw ML scores in Tooltip), and a calm desktop Quick Preview pane.
 */

export function SearchInterface() {
  const inRouter = useInRouterContext();
  return inRouter ? <SearchInterfaceWithRouter /> : <SearchInterfaceBody initialUrlQuery="" />;
}

function SearchInterfaceWithRouter() {
  const [searchParams] = useSearchParams();
  const location = useLocation();
  const navigate = useNavigate();
  const urlQuery = searchParams.get('q') ?? '';

  const handleAskInChat = useCallback(
    (result: SearchResult) => {
      const documentId = result.documentId || result.id;
      if (!documentId) return;
      void navigate(`/chat?${new URLSearchParams({ new: '1', documentId }).toString()}`);
    },
    [navigate],
  );

  return (
    <SearchInterfaceBody
      initialUrlQuery={urlQuery}
      syncedUrlQuery={urlQuery}
      locationKey={location.key}
      onAskInChat={handleAskInChat}
    />
  );
}

interface SearchInterfaceBodyProps {
  initialUrlQuery: string;
  syncedUrlQuery?: string;
  locationKey?: string;
  onAskInChat?: (result: SearchResult) => void;
}

function SearchInterfaceBody({
  initialUrlQuery,
  syncedUrlQuery,
  locationKey,
  onAskInChat,
}: SearchInterfaceBodyProps) {
  const api = useApi();
  const isWideScreen = useIsWideScreen();
  const [query, setQuery] = useState(initialUrlQuery);
  const lastSyncRef = useRef({ syncedUrlQuery, locationKey });

  useEffect(() => {
    const prev = lastSyncRef.current;
    if (prev.syncedUrlQuery === syncedUrlQuery && prev.locationKey === locationKey) {
      return;
    }
    lastSyncRef.current = { syncedUrlQuery, locationKey };
    if (syncedUrlQuery !== undefined && (syncedUrlQuery !== '' || prev.syncedUrlQuery)) {
      setQuery(syncedUrlQuery);
    }
  }, [syncedUrlQuery, locationKey]);

  const [searchMode, setSearchMode] = useState<SearchMode>('hybrid');
  const [activeType, setActiveType] = useState<string | null>(null);
  const [activeSource, setActiveSource] = useState<string | null>(null);
  const [selectedResultId, setSelectedResultId] = useState<string | null>(null);
  const [previewTarget, setPreviewTarget] = useState<ResolvedReaderTarget | null>(null);
  const [openError, setOpenError] = useState<string | null>(null);

  const debouncedQuery = useDebounce(query, 300);

  const {
    data: results = [],
    isLoading: isSearching,
    isFetching,
    error,
    refetch,
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

  const facets = useMemo(() => extractSearchFacets(groupedResults), [groupedResults]);

  // Clear stale filters when the underlying result set changes.
  useEffect(() => {
    if (activeType && !facets.types.some((item) => item.value === activeType)) {
      setActiveType(null);
    }
    if (activeSource && !facets.sources.some((item) => item.value === activeSource)) {
      setActiveSource(null);
    }
  }, [facets, activeType, activeSource]);

  const filteredResults = useMemo(
    () => filterSearchResults(groupedResults, activeType, activeSource),
    [groupedResults, activeType, activeSource],
  );

  const hasActiveFilters = Boolean(activeType || activeSource);
  const showFacetBar =
    groupedResults.length > 0 &&
    (facets.types.length > 1 || facets.sources.length > 1 || hasActiveFilters);

  const selectedResult = useMemo(() => {
    if (filteredResults.length === 0) return null;
    if (selectedResultId) {
      const match = filteredResults.find((item) => item.id === selectedResultId);
      if (match) return match;
    }
    return filteredResults[0] ?? null;
  }, [filteredResults, selectedResultId]);

  const showTwoPane = isWideScreen && filteredResults.length > 0 && selectedResult !== null;

  const handleQueryChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      setQuery(e.target.value);
    },
    []
  );

  const handleClearQuery = useCallback(() => {
    setQuery('');
    setActiveType(null);
    setActiveSource(null);
    setSelectedResultId(null);
  }, []);

  const handleResultSelect = useCallback((result: SearchResult) => {
    setSelectedResultId(result.id);
  }, []);

  const handleResultOpen = useCallback(async (result: SearchResult) => {
    setOpenError(null);

    const target = fromSearchResult(result);
    if (!target.source.documentId && !target.source.filePath) {
      setOpenError('Result has no document identifier');
      return;
    }

    if (/^https?:\/\//i.test(target.source.filePath)) {
      setPreviewTarget(target);
      return;
    }

    const targetId = result.documentId || result.id || target.source.documentId;
    if (targetId) {
      const byId = await openDocumentFile(targetId, api);
      if (byId.ok) {
        if (byId.data.action === 'render_internal') {
          setPreviewTarget(target);
        }
        return;
      }

      if (typeof result.path === 'string' && result.path.length > 0) {
        const byPath = await openFilePath(result.path, api);
        if (byPath.ok) {
          if (byPath.data.action === 'render_internal') {
            setPreviewTarget(target);
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
  }, [api]);

  const resultMetaLabel = useMemo(() => {
    if (groupedResults.length === 0) return undefined;
    if (hasActiveFilters) {
      return `${filteredResults.length} of ${groupedResults.length} results`;
    }
    return `${groupedResults.length} ${groupedResults.length === 1 ? 'result' : 'results'}`;
  }, [filteredResults.length, groupedResults.length, hasActiveFilters]);

  return (
    <TooltipProvider delayDuration={180}>
      <main className="h-full overflow-y-auto bg-bg">
        <div
          className={cn(
            'mx-auto w-full px-6 pt-10 pb-16 transition-[max-width] duration-base',
            showTwoPane ? 'max-w-[1140px]' : 'max-w-[760px]',
          )}
        >
          <PageHeader title="Search" meta={resultMetaLabel} />

          <div className="max-w-[760px]">
            <SearchInput
              value={query}
              onChange={handleQueryChange}
              onClear={handleClearQuery}
              isSearching={isSearching}
            />

            <div className="mt-3 flex flex-wrap items-center justify-between gap-3">
              <Tabs
                variant="pill"
                value={searchMode}
                onValueChange={(value) => setSearchMode(value as SearchMode)}
              >
                <TabsList aria-label="Search mode">
                  {SEARCH_MODES.map((mode) => (
                    <TabsTrigger key={mode.id} value={mode.id}>
                      {mode.label}
                    </TabsTrigger>
                  ))}
                </TabsList>
              </Tabs>
            </div>

            {showFacetBar && (
              <div
                className="mt-3.5 flex flex-wrap items-center gap-1.5"
                role="group"
                aria-label="Filter search results"
              >
                {facets.types.length > 1 &&
                  facets.types.map((facet) => {
                    const active = activeType === facet.value;
                    return (
                      <button
                        key={`type-${facet.value}`}
                        type="button"
                        aria-pressed={active}
                        onClick={() => setActiveType(active ? null : facet.value)}
                        className={cn(
                          'inline-flex h-7 items-center gap-1.5 rounded-full border px-2.5 text-xs font-medium transition-colors duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                          active
                            ? 'border-action bg-action text-action-fg'
                            : 'border-border-default bg-surface text-text-secondary hover:border-border-strong hover:text-text-primary',
                        )}
                      >
                        <span>{facet.value}</span>
                        <span
                          className={cn(
                            'tabular-nums text-[11px]',
                            active ? 'text-action-fg/75' : 'text-text-muted',
                          )}
                        >
                          {facet.count}
                        </span>
                      </button>
                    );
                  })}

                {facets.types.length > 1 && facets.sources.length > 1 && (
                  <span aria-hidden="true" className="mx-1 h-4 w-px bg-border-subtle" />
                )}

                {facets.sources.length > 1 &&
                  facets.sources.map((facet) => {
                    const active = activeSource === facet.value;
                    return (
                      <button
                        key={`source-${facet.value}`}
                        type="button"
                        aria-pressed={active}
                        onClick={() => setActiveSource(active ? null : facet.value)}
                        className={cn(
                          'inline-flex h-7 max-w-[180px] items-center gap-1.5 rounded-full border px-2.5 text-xs font-medium transition-colors duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
                          active
                            ? 'border-accent/50 bg-accent-muted text-text-primary'
                            : 'border-border-subtle bg-transparent text-text-secondary hover:border-border-default hover:bg-surface hover:text-text-primary',
                        )}
                      >
                        <span className="truncate">{facet.value}</span>
                        <span className="shrink-0 tabular-nums text-[11px] text-text-muted">
                          {facet.count}
                        </span>
                      </button>
                    );
                  })}

                {hasActiveFilters && (
                  <button
                    type="button"
                    onClick={() => {
                      setActiveType(null);
                      setActiveSource(null);
                    }}
                    className="ml-1 inline-flex h-7 items-center gap-1 rounded-full px-2 text-xs text-text-tertiary transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
                  >
                    <X className="h-3 w-3" strokeWidth={1.75} aria-hidden="true" />
                    Clear
                  </button>
                )}
              </div>
            )}
          </div>

          {error && (
            <p className="mt-4 max-w-[760px] text-sm text-danger-fg">
              <span>Couldn&apos;t search. {error.message}</span>{' '}
              <button
                type="button"
                onClick={() => void refetch()}
                disabled={isFetching}
                className="font-medium underline underline-offset-2 disabled:opacity-60"
              >
                Try again
              </button>
            </p>
          )}
          {openError && <p className="mt-4 max-w-[760px] text-sm text-danger-fg">{openError}</p>}

          <div className="mt-6">
            {groupedResults.length === 0 ? (
              // A failed search found nothing because it never looked; saying
              // "no results" beside the failure would read as an empty search.
              error ? null : (
                <SearchEmptyState
                  hasSearched={hasSearched}
                  query={debouncedQuery}
                  isSearching={isSearching}
                  onPickRecent={setQuery}
                />
              )
            ) : filteredResults.length === 0 ? (
              <div className="flex max-w-[760px] items-center justify-between gap-4 rounded-xl border border-border-subtle bg-surface px-5 py-4">
                <div>
                  <p className="text-sm font-medium text-text-primary">
                    No results match the selected filters.
                  </p>
                  <p className="mt-0.5 text-xs text-text-secondary">
                    Reset filters to view all {groupedResults.length} matches.
                  </p>
                </div>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    setActiveType(null);
                    setActiveSource(null);
                  }}
                >
                  Reset filters
                </Button>
              </div>
            ) : (
              <div
                className={cn(
                  showTwoPane
                    ? 'grid grid-cols-[minmax(0,1fr)_360px] items-start gap-7'
                    : 'max-w-[760px]',
                )}
              >
                <VirtualizedSearchResults
                  results={filteredResults}
                  query={debouncedQuery}
                  selectedId={showTwoPane ? selectedResult?.id ?? null : null}
                  showQuickPreviewAction={showTwoPane}
                  onResultSelect={handleResultSelect}
                  onResultOpen={handleAsyncEvent(handleResultOpen)}
                />

                {showTwoPane && selectedResult && (
                  <SearchPreviewPane
                    result={selectedResult}
                    query={debouncedQuery}
                    onOpen={handleAsyncEvent(handleResultOpen)}
                    onAskInChat={onAskInChat}
                  />
                )}
              </div>
            )}
          </div>
        </div>

        <FilePreviewModal
          isOpen={previewTarget !== null}
          onClose={() => setPreviewTarget(null)}
          source={previewTarget?.source ?? null}
          initialLocator={previewTarget?.locator ?? null}
        />
      </main>
    </TooltipProvider>
  );
}

SearchInterface.displayName = 'SearchInterface';
