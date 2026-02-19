import { useState, useCallback, useEffect, useRef } from 'react';

import { useDebounce } from './useDebounce';
import VaultAPI from '../lib/api';

import type { SearchResult } from '../types';

interface UseSearchDataProps {
  initialMode?: 'semantic' | 'keyword' | 'hybrid';
  debounceMs?: number;
  limit?: number;
}

interface UseSearchDataReturn {
  query: string;
  results: SearchResult[];
  isSearching: boolean;
  error: string | null;
  searchMode: 'semantic' | 'keyword' | 'hybrid';
  hasSearched: boolean;
  setQuery: (_query: string) => void;
  setSearchMode: (_mode: 'semantic' | 'keyword' | 'hybrid') => void;
  performSearch: (_searchQuery: string) => Promise<void>;
  clearResults: () => void;
}

export function useSearchData({
  initialMode = 'hybrid',
  debounceMs = 300,
  limit = 20,
}: UseSearchDataProps = {}): UseSearchDataReturn {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchResult[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searchMode, setSearchMode] = useState<'semantic' | 'keyword' | 'hybrid'>(initialMode);
  const [hasSearched, setHasSearched] = useState(false);
  const abortControllerRef = useRef<AbortController | null>(null);

  const debouncedQuery = useDebounce(query, debounceMs);

  const performSearch = useCallback(
    async (searchQuery: string) => {
      if (!searchQuery.trim()) {
        setResults([]);
        setHasSearched(false);
        return;
      }

      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }

      const controller = new AbortController();
      abortControllerRef.current = controller;

      setIsSearching(true);
      setError(null);
      setHasSearched(true);

      try {
        const result = await VaultAPI.searchHybrid(
          searchQuery,
          limit,
          searchMode
        );

        if (!controller.signal.aborted) {
          if (result.ok) {
            setResults(result.data);
          } else {
            const errorMsg = result.error || 'Search failed';
            setError(errorMsg);
            setResults([]);
            console.error('[useSearchData] Search failed:', result.error);
          }
          setIsSearching(false);
        }
      } catch (err) {
        if (!controller.signal.aborted) {
          const errorMsg = err instanceof Error ? err.message : 'Search failed. Please try again.';
          setError(errorMsg);
          setResults([]);
          setIsSearching(false);
          console.error('[useSearchData] Search error:', err);
        }
      }
    },
    [searchMode, limit]
  );

  useEffect(() => {
    performSearch(debouncedQuery);

    return () => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }
    };
  }, [debouncedQuery, performSearch]);

  const clearResults = useCallback(() => {
    setResults([]);
    setQuery('');
    setHasSearched(false);
    setError(null);
  }, []);

  return {
    query,
    results,
    isSearching,
    error,
    searchMode,
    hasSearched,
    setQuery,
    setSearchMode,
    performSearch,
    clearResults,
  };
}
