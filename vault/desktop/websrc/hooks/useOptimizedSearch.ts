import { useState, useCallback, useMemo, useRef } from 'react';

import { VaultAPI } from '../lib/api';
import { logger } from '../utils/logger';

import type { SearchResult } from '../types';

interface SearchOptions {
  limit?: number;
  mode?: 'semantic' | 'keyword' | 'hybrid';
  debounceMs?: number;
}

interface UseOptimizedSearchResult {
  results: SearchResult[];
  isSearching: boolean;
  error: string | null;
  search: (query: string) => void;
  clearResults: () => void;
}

/**
 * Optimized search hook with memoization and debouncing
 * Prevents unnecessary re-renders and API calls
 */
export function useOptimizedSearch(options: SearchOptions = {}): UseOptimizedSearchResult {
  const {
    limit = 20,
    mode = 'hybrid',
    debounceMs = 300
  } = options;

  const [results, setResults] = useState<SearchResult[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const searchTimeoutRef = useRef<number | null>(null);
  const abortControllerRef = useRef<AbortController | null>(null);

  // Memoized search function
  const search = useCallback((query: string) => {
    // Clear existing timeout
    if (searchTimeoutRef.current) {
      clearTimeout(searchTimeoutRef.current);
    }

    // Abort existing request
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }

    if (!query.trim() || query.length < 2) {
      setResults([]);
      setIsSearching(false);
      setError(null);
      return;
    }

    setIsSearching(true);
    setError(null);

    // Create new abort controller
    const abortController = new AbortController();
    abortControllerRef.current = abortController;

    // Debounced search
    searchTimeoutRef.current = window.setTimeout(async () => {
      try {
        // Use VaultAPI search methods based on mode
        const result = mode === 'semantic'
          ? await VaultAPI.searchSemantic(query, limit)
          : mode === 'keyword'
          ? await VaultAPI.searchFast(query, limit)
          : await VaultAPI.searchHybrid(query, limit);

        if (!abortController.signal.aborted) {
          if (result.ok) {
            setResults(result.data);
            setIsSearching(false);
          } else {
            setError(result.error);
            setIsSearching(false);
            logger.error('Search failed:', { error: result.error });
          }
        }
      } catch (err) {
        if (!abortController.signal.aborted) {
          const errorMessage = err instanceof Error ? err.message : 'Search failed';
          setError(errorMessage);
          setIsSearching(false);
          logger.error('Search failed:', { error: err });
        }
      }
    }, debounceMs);
  }, [limit, mode, debounceMs]);

  // Memoized clear function
  const clearResults = useCallback(() => {
    setResults([]);
    setError(null);
    setIsSearching(false);

    if (searchTimeoutRef.current) {
      clearTimeout(searchTimeoutRef.current);
    }

    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
  }, []);

  // Memoized result object
  return useMemo(() => ({
    results,
    isSearching,
    error,
    search,
    clearResults,
  }), [results, isSearching, error, search, clearResults]);
}