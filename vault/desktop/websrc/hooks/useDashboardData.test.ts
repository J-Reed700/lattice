/**
 * Tests for useDashboardData Hook
 *
 * Purpose: Comprehensive testing of dashboard data fetching and auto-refresh
 */

import { renderHook, waitFor, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

import { useDashboardData } from './useDashboardData';

import type { IndexingStats, IndexingActivity, DocumentMetadata, ApiResult } from '../types';

// Create mock functions
const mockGetIndexingStats = vi.fn();
const mockListAllDocuments = vi.fn();
const mockGetIndexingActivities = vi.fn();

// Mock VaultAPI module (matching the actual import pattern)
vi.mock('../lib/api', () => ({
  default: {
    getIndexingStats: () => mockGetIndexingStats(),
    listAllDocuments: (limit: number) => mockListAllDocuments(limit),
    getIndexingActivities: (limit: number) => mockGetIndexingActivities(limit),
  }
}));

describe('useDashboardData', () => {
  // Mock data
  const mockStats: IndexingStats = {
    indexedDocuments: 42,
    totalChunks: 350,
  };

  const mockDocuments: DocumentMetadata[] = [
    {
      id: '1',
      fileName: 'doc1.txt',
      filePath: '/path/to/doc1.txt',
      fileType: 'txt',
      category: 'document',
      language: 'en',
      modifiedAt: '2024-01-01T00:00:00Z',
      indexedAt: '2024-01-01T00:00:00Z',
      wordCount: 100,
    },
  ];

  const mockActivities: IndexingActivity[] = [
    {
      id: '1',
      action: 'indexed',
      file_path: '/path/to/doc1.txt',
      status: 'success',
      timestamp: '2024-01-01T00:00:00Z',
      details: null,
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.clearAllTimers();
  });

  describe('Initialization', () => {
    it('should initialize with loading state', () => {
      mockGetIndexingStats.mockReturnValue(new Promise(() => {}));
      mockListAllDocuments.mockReturnValue(new Promise(() => {}));
      mockGetIndexingActivities.mockReturnValue(new Promise(() => {}));

      const { result } = renderHook(() => useDashboardData(0));

      expect(result.current.loading).toBe(true);
      expect(result.current.data).toBeNull();
      expect(result.current.error).toBeNull();
    });
  });

  describe('Data Fetching', () => {
    it('should fetch dashboard data successfully', async () => {
      mockGetIndexingStats.mockResolvedValue({
        ok: true,
        data: mockStats,
      } as ApiResult<IndexingStats>);
      mockListAllDocuments.mockResolvedValue({
        ok: true,
        data: mockDocuments,
      } as ApiResult<DocumentMetadata[]>);
      mockGetIndexingActivities.mockResolvedValue({
        ok: true,
        data: mockActivities,
      } as ApiResult<IndexingActivity[]>);

      const { result } = renderHook(() => useDashboardData(0));

      await waitFor(() => expect(result.current.loading).toBe(false), {
        timeout: 3000,
      });

      expect(result.current.data).not.toBeNull();
      expect(result.current.data?.stats).toEqual(mockStats);
      expect(result.current.error).toBeNull();
    });

    it('should transform recent documents correctly', async () => {
      mockGetIndexingStats.mockResolvedValue({ ok: true, data: mockStats });
      mockListAllDocuments.mockResolvedValue({
        ok: true,
        data: mockDocuments,
      });
      mockGetIndexingActivities.mockResolvedValue({
        ok: true,
        data: mockActivities,
      });

      const { result } = renderHook(() => useDashboardData(0));

      await waitFor(() => expect(result.current.loading).toBe(false));

      expect(result.current.data?.recentDocuments).toHaveLength(1);
      expect(result.current.data?.recentDocuments[0].fileName).toBe('doc1.txt');
    });
  });

  describe('Error Handling', () => {
    it('should handle stats API error', async () => {
      mockGetIndexingStats.mockResolvedValue({
        ok: false,
        error: 'Failed to fetch stats',
      });
      mockListAllDocuments.mockResolvedValue({ ok: true, data: [] });
      mockGetIndexingActivities.mockResolvedValue({ ok: true, data: [] });

      const { result } = renderHook(() => useDashboardData(0));

      await waitFor(() => expect(result.current.loading).toBe(false));

      expect(result.current.error).toBe('Failed to load stats: Failed to fetch stats');
      expect(result.current.data).toBeNull();
    });

    it('should handle exceptions', async () => {
      mockGetIndexingStats.mockRejectedValue(new Error('Network error'));

      const { result } = renderHook(() => useDashboardData(0));

      await waitFor(() => expect(result.current.loading).toBe(false));

      expect(result.current.error).toBe('Network error');
    });
  });

  describe('isEmpty calculation', () => {
    it('should be true when no documents indexed', async () => {
      mockGetIndexingStats.mockResolvedValue({
        ok: true,
        data: { indexedDocuments: 0, totalChunks: 0 },
      });
      mockListAllDocuments.mockResolvedValue({ ok: true, data: [] });
      mockGetIndexingActivities.mockResolvedValue({ ok: true, data: [] });

      const { result } = renderHook(() => useDashboardData(0));

      await waitFor(() => expect(result.current.loading).toBe(false));

      expect(result.current.isEmpty).toBe(true);
    });

    it('should be false when documents exist', async () => {
      mockGetIndexingStats.mockResolvedValue({
        ok: true,
        data: { indexedDocuments: 5, totalChunks: 50 },
      });
      mockListAllDocuments.mockResolvedValue({ ok: true, data: [] });
      mockGetIndexingActivities.mockResolvedValue({ ok: true, data: [] });

      const { result } = renderHook(() => useDashboardData(0));

      await waitFor(() => expect(result.current.loading).toBe(false));

      expect(result.current.isEmpty).toBe(false);
    });
  });

  describe('refetch', () => {
    it('should refetch data when called', async () => {
      mockGetIndexingStats.mockResolvedValue({ ok: true, data: mockStats });
      mockListAllDocuments.mockResolvedValue({ ok: true, data: [] });
      mockGetIndexingActivities.mockResolvedValue({ ok: true, data: [] });

      const { result } = renderHook(() => useDashboardData(0));

      await waitFor(() => expect(result.current.loading).toBe(false));

      expect(mockGetIndexingStats).toHaveBeenCalledTimes(1);

      await act(async () => {
        await result.current.refetch();
      });

      expect(mockGetIndexingStats).toHaveBeenCalledTimes(2);
    });
  });
});
