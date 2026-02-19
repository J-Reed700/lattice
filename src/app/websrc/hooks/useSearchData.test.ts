/**
 * Tests for useSearchData Hook
 *
 * Purpose: Comprehensive testing of search functionality with debouncing and cancellation
 */

import { renderHook, waitFor, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { useSearchData } from './useSearchData';

import type { SearchResult, ApiResult } from '../types';

// Create mock function
const mockSearchHybrid = vi.fn();

// Mock useDebounce to pass through immediately
vi.mock('./useDebounce', () => ({
  useDebounce: (value: unknown) => value,
}));

// Mock VaultAPI (matching the actual import pattern)
vi.mock('../lib/api', () => ({
  default: {
    searchHybrid: (query: string, limit: number, mode: string) =>
      mockSearchHybrid(query, limit, mode),
  }
}));

describe('useSearchData', () => {
  const mockResults: SearchResult[] = [
    {
      id: '1',
      title: 'Test Result',
      content: 'Machine learning basics',
      metadata: {
        path: '/docs/ml.txt',
        filename: 'ml.txt',
        file_type: 'txt',
        file_size: 1024,
        updated_at: '2024-01-01',
        created_at: '2024-01-01'
      },
      score: 0.95,
      path: null,
      documentId: null,
      position: null,
      vectorScore: null,
      bm25Score: null,
      vectorRank: null,
      bm25Rank: null,
    },
    {
      id: '2',
      title: 'Test Result',
      content: 'Deep learning guide',
      metadata: {
        path: '/docs/dl.txt',
        filename: 'dl.txt',
        file_type: 'txt',
        file_size: 1024,
        updated_at: '2024-01-01',
        created_at: '2024-01-01'
      },
      score: 0.88,
      path: null,
      documentId: null,
      position: null,
      vectorScore: null,
      bm25Score: null,
      vectorRank: null,
      bm25Rank: null,
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('Initialization', () => {
    it('should initialize with empty state', () => {
      const { result } = renderHook(() => useSearchData());

      expect(result.current.query).toBe('');
      expect(result.current.results).toEqual([]);
      expect(result.current.isSearching).toBe(false);
      expect(result.current.error).toBeNull();
      expect(result.current.searchMode).toBe('hybrid');
      expect(result.current.hasSearched).toBe(false);
    });

    it('should initialize with custom mode', () => {
      const { result } = renderHook(() =>
        useSearchData({ initialMode: 'semantic' })
      );

      expect(result.current.searchMode).toBe('semantic');
    });
  });

  describe('Search Execution', () => {
    it('should perform search with valid query', async () => {
      mockSearchHybrid.mockResolvedValue({
        ok: true,
        data: mockResults,
      } as ApiResult<SearchResult[]>);

      const { result } = renderHook(() => useSearchData());

      act(() => {
        result.current.setQuery('machine learning');
      });

      await waitFor(() => expect(result.current.isSearching).toBe(false));

      expect(mockSearchHybrid).toHaveBeenCalledWith(
        'machine learning',
        20,
        'hybrid'
      );
      expect(result.current.results).toEqual(mockResults);
      expect(result.current.hasSearched).toBe(true);
    });

    it('should handle different search modes', async () => {
      mockSearchHybrid.mockResolvedValue({ ok: true, data: mockResults });

      const { result } = renderHook(() => useSearchData());

      act(() => {
        result.current.setSearchMode('semantic');
      });

      act(() => {
        result.current.setQuery('test query');
      });

      await waitFor(() => expect(mockSearchHybrid).toHaveBeenCalled());

      expect(mockSearchHybrid).toHaveBeenCalledWith(
        'test query',
        20,
        'semantic'      );
    });

    it('should use performSearch method', async () => {
      mockSearchHybrid.mockResolvedValue({ ok: true, data: mockResults });

      const { result } = renderHook(() => useSearchData());

      await act(async () => {
        await result.current.performSearch('direct search');
      });

      expect(mockSearchHybrid).toHaveBeenCalledWith(
        'direct search',
        20,
        'hybrid'      );
      expect(result.current.results).toEqual(mockResults);
    });
  });

  describe('Error Handling', () => {
    it('should handle API errors', async () => {
      mockSearchHybrid.mockResolvedValue({
        ok: false,
        error: 'Search failed: timeout',
      });

      const { result } = renderHook(() => useSearchData());

      act(() => {
        result.current.setQuery('test query');
      });

      await waitFor(() => expect(result.current.isSearching).toBe(false));

      expect(result.current.error).toBe('Search failed: timeout');
      expect(result.current.results).toEqual([]);
    });

    it('should handle exceptions', async () => {
      mockSearchHybrid.mockRejectedValue(new Error('Network error'));

      const { result } = renderHook(() => useSearchData());

      act(() => {
        result.current.setQuery('test query');
      });

      await waitFor(() => expect(result.current.isSearching).toBe(false));

      expect(result.current.error).toBe('Network error');
      expect(result.current.results).toEqual([]);
    });

    it('should clear error on successful search', async () => {
      mockSearchHybrid
        .mockResolvedValueOnce({ ok: false, error: 'Search failed' })
        .mockResolvedValueOnce({ ok: true, data: mockResults });

      const { result } = renderHook(() => useSearchData());

      act(() => {
        result.current.setQuery('test1');
      });

      await waitFor(() => expect(result.current.error).toBe('Search failed'));

      act(() => {
        result.current.setQuery('test2');
      });

      await waitFor(() => expect(result.current.error).toBeNull());
      expect(result.current.results).toEqual(mockResults);
    });
  });

  describe('Edge Cases', () => {
    it('should handle empty query', () => {
      const { result } = renderHook(() => useSearchData());

      act(() => {
        result.current.setQuery('');
      });

      expect(mockSearchHybrid).not.toHaveBeenCalled();
      expect(result.current.results).toEqual([]);
      expect(result.current.hasSearched).toBe(false);
    });

    it('should handle whitespace-only query', () => {
      const { result } = renderHook(() => useSearchData());

      act(() => {
        result.current.setQuery('   ');
      });

      expect(mockSearchHybrid).not.toHaveBeenCalled();
      expect(result.current.results).toEqual([]);
    });

    it('should handle no results', async () => {
      mockSearchHybrid.mockResolvedValue({ ok: true, data: [] });

      const { result } = renderHook(() => useSearchData());

      act(() => {
        result.current.setQuery('no results query');
      });

      await waitFor(() => expect(result.current.isSearching).toBe(false));

      expect(result.current.results).toEqual([]);
      expect(result.current.hasSearched).toBe(true);
      expect(result.current.error).toBeNull();
    });

    it('should handle special characters in query', async () => {
      mockSearchHybrid.mockResolvedValue({ ok: true, data: mockResults });

      const { result } = renderHook(() => useSearchData());

      const specialQuery = 'test & query | with * special ? chars';

      act(() => {
        result.current.setQuery(specialQuery);
      });

      await waitFor(() => expect(mockSearchHybrid).toHaveBeenCalled());

      expect(mockSearchHybrid).toHaveBeenCalledWith(
        specialQuery,
        20,
        'hybrid'
      );
    });
  });

  describe('Clear Results', () => {
    it('should clear results when clearResults is called', async () => {
      mockSearchHybrid.mockResolvedValue({ ok: true, data: mockResults });

      const { result } = renderHook(() => useSearchData());

      act(() => {
        result.current.setQuery('test');
      });

      await waitFor(() => expect(result.current.results).toEqual(mockResults));

      act(() => {
        result.current.clearResults();
      });

      expect(result.current.query).toBe('');
      expect(result.current.results).toEqual([]);
      expect(result.current.hasSearched).toBe(false);
      expect(result.current.error).toBeNull();
    });

    it('should maintain searchMode when clearing results', async () => {
      mockSearchHybrid.mockResolvedValue({ ok: true, data: mockResults });

      const { result } = renderHook(() => useSearchData());

      act(() => {
        result.current.setSearchMode('semantic');
        result.current.setQuery('test');
      });

      await waitFor(() => expect(result.current.results).toEqual(mockResults));

      act(() => {
        result.current.clearResults();
      });

      expect(result.current.searchMode).toBe('semantic');
    });
  });

  describe('State Management', () => {
    it('should track hasSearched correctly', async () => {
      mockSearchHybrid.mockResolvedValue({ ok: true, data: mockResults });

      const { result } = renderHook(() => useSearchData());

      expect(result.current.hasSearched).toBe(false);

      act(() => {
        result.current.setQuery('test');
      });

      await waitFor(() => expect(result.current.hasSearched).toBe(true));
    });

    it('should reset hasSearched on empty query', async () => {
      mockSearchHybrid.mockResolvedValue({ ok: true, data: mockResults });

      const { result } = renderHook(() => useSearchData());

      act(() => {
        result.current.setQuery('test');
      });

      await waitFor(() => expect(result.current.hasSearched).toBe(true));

      act(() => {
        result.current.setQuery('');
      });

      expect(result.current.hasSearched).toBe(false);
    });

    it('should update search mode dynamically', async () => {
      mockSearchHybrid.mockResolvedValue({ ok: true, data: mockResults });

      const { result } = renderHook(() => useSearchData());

      act(() => {
        result.current.setQuery('test');
      });

      await waitFor(() => expect(mockSearchHybrid).toHaveBeenCalledTimes(1));

      act(() => {
        result.current.setSearchMode('semantic');
        result.current.setQuery('test2');
      });

      await waitFor(() => expect(mockSearchHybrid).toHaveBeenCalledTimes(2));

      expect(mockSearchHybrid).toHaveBeenLastCalledWith(
        'test2',
        20,
        'semantic'
      );
    });
  });
});
