import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { deferred } from '@/tests/deferred';

import { INDEXED_FOLDERS_QUERY_KEY } from './useIndexedFoldersQuery';
import {
  INDEXING_ACTIVITIES_QUERY_KEY,
  INDEXING_STATUS_QUERY_KEY,
  isIndexingActive,
  useIndexingActivitiesQuery,
  useIndexingControlMutation,
  useIndexingStatusQuery,
  useRemoveIndexedFileMutation,
  useRetryIndexedFileMutation,
} from './useIndexingStatusQuery';

const h = vi.hoisted(() => ({
  api: {
    getIndexProgress: vi.fn(),
    getIndexingActivities: vi.fn(),
    pauseIndexing: vi.fn(),
    resumeIndexing: vi.fn(),
    cancelIndexing: vi.fn(),
    indexFile: vi.fn(),
    reindexFile: vi.fn(),
    clearIndexingFailure: vi.fn(),
    removeIndexedFile: vi.fn(),
  },
}));
vi.mock('@/lib/api', () => ({ default: h.api, VaultAPI: h.api }));

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
const ok = <T,>(data: T) => ({ ok: true as const, data });
const refused = (error: string) => ({ ok: false as const, error });

const snapshot = { totalFiles: 10, processed: 2, failed: 1, status: 'processing', percentage: 20, paused: false, failures: [{ path: '/a.pdf' }] };

describe('useIndexingStatusQuery', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
    vi.spyOn(client, 'invalidateQueries');
    for (const fn of Object.values(h.api)) fn.mockResolvedValue(ok(undefined));
  });

  it('knows which states are active', () => {
    expect(isIndexingActive(undefined)).toBe(false);
    expect(isIndexingActive({ status: 'scanning' } as never)).toBe(true);
    expect(isIndexingActive({ status: 'complete' } as never)).toBe(false);
  });

  it('reads progress and merges live events without dropping failures', async () => {
    let emit!: (event: { payload: unknown }) => void;
    const unlisten = vi.fn();
    vi.mocked(listen).mockImplementationOnce(async (_name, handler) => { emit = handler as never; return unlisten; });
    h.api.getIndexProgress.mockResolvedValueOnce(ok(snapshot));
    const { result, unmount } = renderHook(() => useIndexingStatusQuery(), { wrapper });
    await waitFor(() => expect(result.current.data).toEqual(snapshot));
    act(() => emit({ payload: { processed: 5, percentage: 50 } }));
    await waitFor(() => expect(result.current.data).toMatchObject({ processed: 5, failures: [{ path: '/a.pdf' }], paused: false }));
    act(() => emit({ payload: { paused: true, failures: [] } }));
    await waitFor(() => expect(result.current.data).toMatchObject({ paused: true, failures: [] }));
    unmount();
    expect(unlisten).toHaveBeenCalled();
  });

  it('starts from an empty snapshot when an event arrives first', async () => {
    let emit!: (event: { payload: unknown }) => void;
    vi.mocked(listen).mockImplementationOnce(async (_name, handler) => { emit = handler as never; return () => undefined; });
    const progress = deferred<unknown>();
    h.api.getIndexProgress.mockReturnValueOnce(progress.promise);
    const { result } = renderHook(() => useIndexingStatusQuery(), { wrapper });
    await waitFor(() => expect(listen).toHaveBeenCalled());
    act(() => emit({ payload: { status: 'scanning' } }));
    await waitFor(() => expect(result.current.data).toEqual({ totalFiles: 0, processed: 0, failed: 0, status: 'scanning', percentage: 0, paused: false, failures: [] }));
    await act(async () => { progress.resolve(ok(snapshot)); await progress.promise; });
  });

  it('stops listening when unmounted before the subscription lands', async () => {
    const subscribed = deferred<() => void>();
    const unlisten = vi.fn();
    vi.mocked(listen).mockReturnValueOnce(subscribed.promise as never);
    const { unmount } = renderHook(() => useIndexingStatusQuery(), { wrapper });
    unmount();
    await act(async () => { subscribed.resolve(unlisten); await subscribed.promise; });
    expect(unlisten).toHaveBeenCalled();
  });

  it('surfaces progress it cannot read', async () => {
    h.api.getIndexProgress.mockResolvedValueOnce(refused('indexer offline'));
    const { result } = renderHook(() => useIndexingStatusQuery(), { wrapper });
    await waitFor(() => expect(result.current.error?.message).toBe('indexer offline'));
  });

  it('reads recent activity only when enabled', async () => {
    h.api.getIndexingActivities.mockResolvedValueOnce(ok([{ id: 1 }])).mockResolvedValueOnce(refused('log locked'));
    const disabled = renderHook(() => useIndexingActivitiesQuery(5, false), { wrapper });
    expect(h.api.getIndexingActivities).not.toHaveBeenCalled();
    disabled.unmount();
    const { result } = renderHook(() => useIndexingActivitiesQuery(5), { wrapper });
    await waitFor(() => expect(result.current.data).toEqual([{ id: 1 }]));
    expect(h.api.getIndexingActivities).toHaveBeenCalledWith(5);
    const failing = renderHook(() => useIndexingActivitiesQuery(7), { wrapper });
    await waitFor(() => expect(failing.result.current.error?.message).toBe('log locked'));
  });

  it.each([
    ['pause', 'pauseIndexing'],
    ['resume', 'resumeIndexing'],
    ['cancel', 'cancelIndexing'],
  ] as const)('sends %s and refreshes the status either way', async (action, method) => {
    const { result } = renderHook(() => useIndexingControlMutation(), { wrapper });
    await act(async () => { await result.current.mutateAsync(action); });
    expect(h.api[method]).toHaveBeenCalled();
    h.api[method].mockResolvedValueOnce(refused('not running'));
    await act(async () => { await expect(result.current.mutateAsync(action)).rejects.toThrow('not running'); });
    expect(vi.mocked(client.invalidateQueries).mock.calls.filter(([f]) => f?.queryKey === INDEXING_STATUS_QUERY_KEY)).toHaveLength(2);
  });

  describe('retrying a failed file', () => {
    it('indexes it afresh and clears its failure', async () => {
      const { result } = renderHook(() => useRetryIndexedFileMutation(), { wrapper });
      await act(async () => { await result.current.mutateAsync('/notes/moss.pdf'); });
      expect(h.api.indexFile).toHaveBeenCalledWith('/notes/moss.pdf');
      expect(h.api.reindexFile).not.toHaveBeenCalled();
      expect(h.api.clearIndexingFailure).toHaveBeenCalledWith('/notes/moss.pdf');
      expect(client.invalidateQueries).toHaveBeenCalledWith({ queryKey: INDEXING_ACTIVITIES_QUERY_KEY });
    });

    it('falls back to a reindex when the file is already indexed', async () => {
      h.api.indexFile.mockResolvedValueOnce(refused('already indexed'));
      h.api.clearIndexingFailure.mockRejectedValueOnce(new Error('no such command'));
      const { result } = renderHook(() => useRetryIndexedFileMutation(), { wrapper });
      await act(async () => { await result.current.mutateAsync('/notes/moss.pdf'); });
      expect(h.api.reindexFile).toHaveBeenCalledWith('/notes/moss.pdf');
    });

    it('throws the reindex reason, or the index reason when it has none', async () => {
      const { result } = renderHook(() => useRetryIndexedFileMutation(), { wrapper });
      h.api.indexFile.mockResolvedValueOnce(refused('unreadable'));
      h.api.reindexFile.mockResolvedValueOnce(refused('file locked'));
      await act(async () => { await expect(result.current.mutateAsync('/a')).rejects.toThrow('file locked'); });
      h.api.indexFile.mockResolvedValueOnce(refused('unreadable'));
      h.api.reindexFile.mockResolvedValueOnce(refused(''));
      await act(async () => { await expect(result.current.mutateAsync('/a')).rejects.toThrow('unreadable'); });
    });
  });

  it('removes a file and refreshes status, activity and folders', async () => {
    const { result } = renderHook(() => useRemoveIndexedFileMutation(), { wrapper });
    await act(async () => { await result.current.mutateAsync('/notes/moss.pdf'); });
    expect(h.api.removeIndexedFile).toHaveBeenCalledWith('/notes/moss.pdf');
    expect(client.invalidateQueries).toHaveBeenCalledWith({ queryKey: INDEXED_FOLDERS_QUERY_KEY });
    h.api.removeIndexedFile.mockResolvedValueOnce(refused('in use'));
    await act(async () => { await expect(result.current.mutateAsync('/a')).rejects.toThrow('in use'); });
  });
});
