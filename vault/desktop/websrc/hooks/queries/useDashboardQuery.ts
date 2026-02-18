import { useQuery, type UseQueryOptions } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type { RecentDocument, IndexingActivity } from '@/types';

export interface DashboardData {
  stats: {
    documentCount: number;
    storageUsed: number;
    searchCount: number;
    lastIndexed: string;
  };
  recentDocuments: RecentDocument[];
  recentActivities: IndexingActivity[];
}

export const useDashboardQuery = (
  options?: Omit<UseQueryOptions<DashboardData, Error>, 'queryKey' | 'queryFn'>
) => useQuery<DashboardData, Error>({
    queryKey: ['dashboard'],
    queryFn: async () => {
      // Fetch all dashboard data in parallel
      const [statsResult, docsResult] = await Promise.all([
        VaultAPI.getIndexingStats(),
        VaultAPI.listAllDocuments(10000),
      ]);

      if (!statsResult.ok || !docsResult.ok) {
        throw new Error('Failed to fetch dashboard data');
      }

      // Map IndexingStats to DashboardData.stats format
      const indexingStats = statsResult.data;

      return {
        stats: {
          documentCount: indexingStats?.indexedDocuments || 0,
          storageUsed: 0, // Not available from IndexingStats
          searchCount: 0, // Not available from IndexingStats
          lastIndexed: 'Never', // Not available from IndexingStats
        },
        recentDocuments: (docsResult.data || [])
          .sort((a, b) => new Date(b.indexedAt).getTime() - new Date(a.indexedAt).getTime())
          .slice(0, 10)
          .map(doc => ({
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
          })),
        recentActivities: [],
      };
    },
    staleTime: 30 * 1000,
    refetchInterval: 30 * 1000,
    ...options,
  });
