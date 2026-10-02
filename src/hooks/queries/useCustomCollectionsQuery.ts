import { useMemo } from 'react';

import { useQuery, useQueryClient } from '@tanstack/react-query';

import VaultAPI from '@/lib/api';
import type { CustomCollection } from '@/types/fileBrowser';

export const CUSTOM_COLLECTIONS_QUERY_KEY = ['custom-collections'] as const;
export const LEGACY_CUSTOM_COLLECTIONS_STORAGE_KEY = 'lattice:file-browser-custom-collections:v1';

function readLegacyCollections(): CustomCollection[] | null {
  if (typeof window === 'undefined') return null;
  let raw: string | null;
  try {
    raw = window.localStorage.getItem(LEGACY_CUSTOM_COLLECTIONS_STORAGE_KEY);
  } catch (error) {
    throw new Error(`Unable to read legacy collections: ${String(error)}`);
  }
  if (raw === null) return null;

  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (error) {
    throw new Error(`Legacy collections are not valid JSON; the original data was kept. ${String(error)}`);
  }
  if (!Array.isArray(parsed)) {
    throw new Error('Legacy collections have an unsupported format; the original data was kept.');
  }

  const collections: CustomCollection[] = parsed.map((item: unknown) => {
    if (!item || typeof item !== 'object') throw new Error('A legacy collection is invalid; the original data was kept.');
    const candidate = item as Partial<CustomCollection>;
    if (typeof candidate.id !== 'string' || !candidate.id || typeof candidate.name !== 'string' || !candidate.name.trim()) {
      throw new Error('A legacy collection is incomplete; the original data was kept.');
    }
    const kind = candidate.kind === 'snapshot' ? 'snapshot' : 'manual';
    const documentIds = candidate.documentIds === undefined ? [] : candidate.documentIds;
    if (!Array.isArray(documentIds) || !documentIds.every((id) => typeof id === 'string')) {
      throw new Error('A legacy collection has invalid document membership; the original data was kept.');
    }
    // Stable fallback is important when localStorage removal fails: retries
    // must produce the same import receipt digest.
    const createdAt = typeof candidate.createdAt === 'string' ? candidate.createdAt : '1970-01-01T00:00:00.000Z';
    return {
      id: candidate.id,
      name: candidate.name,
      kind,
      parentId: typeof candidate.parentId === 'string' ? candidate.parentId : null,
      documentIds: Array.from(new Set(documentIds)),
      createdAt,
      updatedAt: typeof candidate.updatedAt === 'string' ? candidate.updatedAt : createdAt,
    };
  });
  const collectionIds = new Set(collections.map((collection) => collection.id));
  return collections.map((collection) => ({
    ...collection,
    parentId: collection.parentId && collectionIds.has(collection.parentId) ? collection.parentId : null,
  }));
}

export async function migrateLegacyCustomCollections(): Promise<void> {
  const legacy = readLegacyCollections();
  if (!legacy) return;
  const imported = await VaultAPI.importLegacyCustomCollections(legacy);
  if (!imported.ok) throw new Error(imported.error);
  // The SQLite transaction has committed. Only now may the sole legacy copy go.
  try {
    window.localStorage.removeItem(LEGACY_CUSTOM_COLLECTIONS_STORAGE_KEY);
  } catch {
    // The import is idempotent, so retaining the key is safe and recoverable.
  }
}

export function useCustomCollectionsQuery() {
  return useQuery<CustomCollection[]>({
    queryKey: CUSTOM_COLLECTIONS_QUERY_KEY,
    queryFn: async () => {
      await migrateLegacyCustomCollections();
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
