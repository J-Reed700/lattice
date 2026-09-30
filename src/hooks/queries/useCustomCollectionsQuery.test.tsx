import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import VaultAPI from '@/lib/api';

import {
  LEGACY_CUSTOM_COLLECTIONS_STORAGE_KEY,
  useCustomCollectionActions,
  useCustomCollectionsQuery,
} from './useCustomCollectionsQuery';

vi.mock('@/lib/api', () => ({ default: {
  importLegacyCustomCollections: vi.fn(),
  listCustomCollections: vi.fn(),
  createCustomCollection: vi.fn(),
  renameCustomCollection: vi.fn(),
  moveCustomCollection: vi.fn(),
  deleteCustomCollection: vi.fn(),
  addDocumentsToCustomCollection: vi.fn(),
  removeDocumentsFromCustomCollection: vi.fn(),
} }));

const wrap = () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return { client, wrapper: ({ children }: { children: React.ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider> };
};

describe('useCustomCollectionsQuery', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
    vi.mocked(VaultAPI.listCustomCollections).mockResolvedValue({ ok: true, data: [] });
    vi.mocked(VaultAPI.importLegacyCustomCollections).mockResolvedValue({ ok: true, data: undefined });
    vi.mocked(VaultAPI.createCustomCollection).mockResolvedValue({ ok: true, data: 'created' });
  });
  afterEach(() => vi.restoreAllMocks());

  it('imports legacy records before listing and removes the legacy key only after successful persistence', async () => {
    localStorage.setItem(LEGACY_CUSTOM_COLLECTIONS_STORAGE_KEY, JSON.stringify([
      { id: 'child', name: 'Child', kind: 'manual', documentIds: ['doc'], parentId: 'missing' },
    ]));
    vi.spyOn(Storage.prototype, 'removeItem').mockImplementation(() => { throw new DOMException('storage full', 'QuotaExceededError'); });
    const { wrapper } = wrap();
    const { result } = renderHook(() => useCustomCollectionsQuery(), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(vi.mocked(VaultAPI.importLegacyCustomCollections).mock.invocationCallOrder[0]).toBeLessThan(
      vi.mocked(VaultAPI.listCustomCollections).mock.invocationCallOrder[0]
    );
    expect(VaultAPI.importLegacyCustomCollections).toHaveBeenCalledWith([
      expect.objectContaining({
        id: 'child', parentId: null, documentIds: ['doc'],
        createdAt: '1970-01-01T00:00:00.000Z', updatedAt: '1970-01-01T00:00:00.000Z',
      }),
    ]);
    expect(localStorage.getItem(LEGACY_CUSTOM_COLLECTIONS_STORAGE_KEY)).not.toBeNull();
  });

  it('removes the legacy key after a successful import', async () => {
    localStorage.setItem(LEGACY_CUSTOM_COLLECTIONS_STORAGE_KEY, JSON.stringify([
      { id: 'one', name: 'One', kind: 'manual', parentId: null, documentIds: [], createdAt: 'a', updatedAt: 'a' },
    ]));
    const { wrapper } = wrap();
    const { result } = renderHook(() => useCustomCollectionsQuery(), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(localStorage.getItem(LEGACY_CUSTOM_COLLECTIONS_STORAGE_KEY)).toBeNull();
  });

  it('keeps legacy data and reports the import error when persistence fails', async () => {
    const raw = JSON.stringify([{ id: 'one', name: 'One', kind: 'manual', parentId: null, documentIds: [], createdAt: 'a', updatedAt: 'a' }]);
    localStorage.setItem(LEGACY_CUSTOM_COLLECTIONS_STORAGE_KEY, raw);
    vi.mocked(VaultAPI.importLegacyCustomCollections).mockResolvedValue({ ok: false, error: 'disk full' });
    const { wrapper } = wrap();
    const { result } = renderHook(() => useCustomCollectionsQuery(), { wrapper });
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(result.current.error?.message).toContain('disk full');
    expect(localStorage.getItem(LEGACY_CUSTOM_COLLECTIONS_STORAGE_KEY)).toBe(raw);
    expect(VaultAPI.listCustomCollections).not.toHaveBeenCalled();
  });

  it('creates concurrently without depending on a warmed query cache', async () => {
    const { wrapper } = wrap();
    const { result } = renderHook(() => useCustomCollectionActions(), { wrapper });
    await act(async () => {
      await Promise.all([result.current.create('First'), result.current.create('Second')]);
    });
    expect(VaultAPI.createCustomCollection).toHaveBeenCalledTimes(2);
  });

  it('does not replay stale legacy data after a committed edit when key removal failed', async () => {
    const raw = JSON.stringify([
      { id: 'one', name: 'One', kind: 'manual', parentId: null, documentIds: ['legacy-doc'], createdAt: 'a', updatedAt: 'a' },
    ]);
    localStorage.setItem(LEGACY_CUSTOM_COLLECTIONS_STORAGE_KEY, raw);
    vi.spyOn(Storage.prototype, 'removeItem').mockImplementation(() => { throw new DOMException('storage blocked', 'QuotaExceededError'); });

    const persisted = new Map<string, { id: string; name: string; kind: 'manual' | 'snapshot'; parentId: string | null; documentIds: string[]; createdAt: string; updatedAt: string }>();
    let importedPayload: string | null = null;
    vi.mocked(VaultAPI.importLegacyCustomCollections).mockImplementation(async (collections) => {
      const digestPayload = JSON.stringify(collections);
      if (importedPayload === digestPayload) return { ok: true, data: undefined };
      importedPayload = digestPayload;
      for (const collection of collections) persisted.set(collection.id, { ...collection, documentIds: [...collection.documentIds] });
      return { ok: true, data: undefined };
    });
    vi.mocked(VaultAPI.listCustomCollections).mockImplementation(async () => ({ ok: true, data: [...persisted.values()] }));
    vi.mocked(VaultAPI.removeDocumentsFromCustomCollection).mockImplementation(async (_id, ids) => {
      const item = persisted.get('one');
      if (item) item.documentIds = item.documentIds.filter((id) => !ids.includes(id));
      return { ok: true, data: undefined };
    });

    const { wrapper } = wrap();
    const { result } = renderHook(() => ({ query: useCustomCollectionsQuery(), actions: useCustomCollectionActions() }), { wrapper });
    await waitFor(() => expect(result.current.query.isSuccess).toBe(true));
    await act(async () => { await result.current.actions.removeDocuments('one', ['legacy-doc']); });
    await waitFor(() => expect(result.current.query.data?.[0]?.documentIds).toEqual([]));
    expect(localStorage.getItem(LEGACY_CUSTOM_COLLECTIONS_STORAGE_KEY)).toBe(raw);
    expect(VaultAPI.importLegacyCustomCollections).toHaveBeenCalledTimes(2);
  });
});
