import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { CUSTOM_COLLECTIONS_QUERY_KEY, useCustomCollectionActions, useCustomCollectionsQuery } from './useCustomCollectionsQuery';

const h = vi.hoisted(() => ({
  api: {
    listCustomCollections: vi.fn(),
    createCustomCollection: vi.fn(),
    renameCustomCollection: vi.fn(),
    moveCustomCollection: vi.fn(),
    deleteCustomCollection: vi.fn(),
    addDocumentsToCustomCollection: vi.fn(),
    removeDocumentsFromCustomCollection: vi.fn(),
  },
}));
vi.mock('@/lib/api', () => ({ default: h.api, VaultAPI: h.api }));

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
const ok = <T,>(data: T) => ({ ok: true as const, data });

describe('custom collections', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    vi.spyOn(client, 'invalidateQueries');
    for (const fn of Object.values(h.api)) fn.mockResolvedValue(ok(null));
  });

  it('lists collections, or fails with the reason', async () => {
    h.api.listCustomCollections.mockResolvedValueOnce(ok([{ id: 'k1', name: 'Herbs' }]));
    const { result } = renderHook(() => useCustomCollectionsQuery(), { wrapper });
    await waitFor(() => expect(result.current.data).toEqual([{ id: 'k1', name: 'Herbs' }]));
    client.clear();
    h.api.listCustomCollections.mockResolvedValueOnce({ ok: false, error: 'legacy data unreadable' });
    const second = renderHook(() => useCustomCollectionsQuery(), { wrapper });
    await waitFor(() => expect(second.result.current.error?.message).toBe('legacy data unreadable'));
  });

  it('creates a manual collection or a snapshot, refusing blanks', async () => {
    const { result } = renderHook(() => useCustomCollectionActions(), { wrapper });
    h.api.createCustomCollection.mockResolvedValueOnce(ok('k1')).mockResolvedValueOnce(ok('k2'));
    await act(async () => { expect(await result.current.create('Herbs')).toBe('k1'); });
    expect(h.api.createCustomCollection).toHaveBeenCalledWith({ name: 'Herbs', kind: 'manual', parentId: null, documentIds: [] });
    await act(async () => { expect(await result.current.create('Snapshot: mint', 'k1', ['d1'])).toBe('k2'); });
    expect(h.api.createCustomCollection).toHaveBeenLastCalledWith({ name: 'Snapshot: mint', kind: 'snapshot', parentId: 'k1', documentIds: ['d1'] });
    await act(async () => {
      expect(await result.current.create('   ')).toBeNull();
      expect(await result.current.create('Empty snapshot', null, [])).toBeNull();
    });
    expect(h.api.createCustomCollection).toHaveBeenCalledTimes(2);
    expect(client.invalidateQueries).toHaveBeenCalledWith({ queryKey: CUSTOM_COLLECTIONS_QUERY_KEY });
  });

  it('adds, removes, moves and deletes', async () => {
    const { result } = renderHook(() => useCustomCollectionActions(), { wrapper });
    await act(async () => {
      await result.current.addDocuments('k1', ['d1']);
      await result.current.removeDocuments('k1', ['d1']);
      await result.current.move('k2', 'k1');
      await result.current.deleteCollection('k2');
    });
    expect(h.api.addDocumentsToCustomCollection).toHaveBeenCalledWith('k1', ['d1']);
    expect(h.api.removeDocumentsFromCustomCollection).toHaveBeenCalledWith('k1', ['d1']);
    expect(h.api.moveCustomCollection).toHaveBeenCalledWith('k2', 'k1');
    expect(h.api.deleteCustomCollection).toHaveBeenCalledWith('k2');
    expect(client.invalidateQueries).toHaveBeenCalledTimes(4);
  });

  it('renames, refusing a blank name', async () => {
    const { result } = renderHook(() => useCustomCollectionActions(), { wrapper });
    await act(async () => {
      expect(await result.current.rename('k1', 'Spices')).toBe(true);
      expect(await result.current.rename('k1', '  ')).toBe(false);
    });
    expect(h.api.renameCustomCollection).toHaveBeenCalledTimes(1);
  });

  it('throws a refused change without refreshing', async () => {
    h.api.deleteCustomCollection.mockResolvedValueOnce({ ok: false, error: 'collection in use' });
    const { result } = renderHook(() => useCustomCollectionActions(), { wrapper });
    await act(async () => { await expect(result.current.deleteCollection('k1')).rejects.toThrow('collection in use'); });
    expect(client.invalidateQueries).not.toHaveBeenCalled();
  });
});
