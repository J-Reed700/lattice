import { useState, useEffect, useCallback } from 'react';

import VaultAPI from '../lib/api';

import type { IndexingStats, IndexingActivity, RecentDocument, DocumentMetadata } from '../types';

interface DashboardData {
  stats: IndexingStats;
  recentDocuments: RecentDocument[];
  recentActivity: IndexingActivity[];
}

interface UseDashboardDataReturn {
  data: DashboardData | null;
  loading: boolean;
  error: string | null;
  refetch: () => Promise<void>;
  isEmpty: boolean;
}

export function useDashboardData(autoRefreshInterval = 30000): UseDashboardDataReturn {
  const [data, setData] = useState<DashboardData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchDashboardData = useCallback(async () => {
    setLoading(true);
    setError(null);

    try {
      const [statsResult, docsResult, activityResult] = await Promise.all([
        VaultAPI.getIndexingStats(),
        VaultAPI.listAllDocuments(10000), // Changed from getRecentDocuments
        VaultAPI.getIndexingActivities(10),
      ]);

      // Early return pattern - no throwing
      if (!statsResult.ok) {
        setError(`Failed to load stats: ${statsResult.error}`);
        setLoading(false);
        return;
      }
      if (!docsResult.ok) {
        setError(`Failed to load documents: ${docsResult.error}`);
        setLoading(false);
        return;
      }
      if (!activityResult.ok) {
        setError(`Failed to load activities: ${activityResult.error}`);
        setLoading(false);
        return;
      }

      // Map DocumentMetadata to RecentDocument format for backwards compatibility
      const recentDocuments: RecentDocument[] = docsResult.data
        .sort((a: DocumentMetadata, b: DocumentMetadata) =>
          new Date(b.indexedAt).getTime() - new Date(a.indexedAt).getTime()
        )
        .slice(0, 10)
        .map((doc: DocumentMetadata) => ({
          id: doc.id,
          documentId: doc.id,
          documentName: doc.fileName,
          documentPath: doc.filePath,
          fileType: doc.fileType,
          lastAccessedAt: doc.indexedAt,
          accessCount: 0,
          // Additional properties used by Dashboard components
          fileName: doc.fileName,
          filePath: doc.filePath,
          indexedAt: doc.indexedAt,
          modifiedAt: doc.modifiedAt,
          sizeBytes: 0, // Not available in DocumentMetadata
        }));

      setData({
        stats: statsResult.data,
        recentDocuments,
        recentActivity: activityResult.data,
      });
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load dashboard data');
    } finally {
      setLoading(false);
    }
  }, []); // setData, setError, setLoading are stable setState functions

  // Initial fetch on mount
  useEffect(() => {
    fetchDashboardData();
  }, [fetchDashboardData]);

  // Auto-refresh interval
  useEffect(() => {
    if (autoRefreshInterval > 0) {
      const interval = setInterval(fetchDashboardData, autoRefreshInterval);
      return () => clearInterval(interval);
    }
  }, [fetchDashboardData, autoRefreshInterval]);

  const isEmpty = data ? data.stats.indexedDocuments === 0 : false;

  return {
    data,
    loading,
    error,
    refetch: fetchDashboardData,
    isEmpty,
  };
}
