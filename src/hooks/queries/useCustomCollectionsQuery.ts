import { useMemo } from 'react';

import { useQuery, useQueryClient } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type { CustomCollection } from '@/types/fileBrowser';

export const CUSTOM_COLLECTIONS_QUERY_KEY = ['custom-collections'] as const;

export function useCustomCollectionsQuery() {
  return useQuery<CustomCollection[]>({
    queryKey: CUSTOM_COLLECTIONS_QUERY_KEY,
    queryFn: async () => {
      const result = await VaultAPI.listCustomCollections();
      if (!result.ok) throw new Error(result.error);
      return result.data;
    },
    staleTime: 30_000,
  });
}

/** Persisted mutations are granular repository operations; writes never depend on query-cache contents. */
export function useCustomCollectionActions() {
  const queryClient = useQueryClient();
  return useMemo(() => {
    const invalidate = async () => { await queryClient.invalidateQueries({ queryKey: CUSTOM_COLLECTIONS_QUERY_KEY }); };
    const run = async <T,>(operation: Promise<{ ok: true; data: T } | { ok: false; error: string }>): Promise<T> => {
      const result = await operation;
      if (!result.ok) throw new Error(result.error);
      await invalidate();
      return result.data;
    };
    return {
      create: (name: string, parentId: string | null = null, snapshotDocumentIds?: string[]) => {
        if (!name.trim() || (snapshotDocumentIds?.length === 0)) return Promise.resolve(null);
        return run(VaultAPI.createCustomCollection({ name, kind: snapshotDocumentIds ? 'snapshot' : 'manual', parentId, documentIds: snapshotDocumentIds ?? [] }));
      },
      addDocuments: (collectionId: string, documentIds: string[]) => run(VaultAPI.addDocumentsToCustomCollection(collectionId, documentIds)),
      removeDocuments: (collectionId: string, documentIds: string[]) => run(VaultAPI.removeDocumentsFromCustomCollection(collectionId, documentIds)),
      rename: async (collectionId: string, name: string) => {
        if (!name.trim()) return false;
        await run(VaultAPI.renameCustomCollection(collectionId, name));
        return true;
      },
      move: (collectionId: string, parentId: string | null) => run(VaultAPI.moveCustomCollection(collectionId, parentId)),
      deleteCollection: (collectionId: string) => run(VaultAPI.deleteCustomCollection(collectionId)),
    };
  }, [queryClient]);
}
