import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { listen } from '@tauri-apps/api/event';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { deferred } from '@/tests/deferred';

import { THEMES_QUERY_KEY, useClustersQuery, useRebuildProgress, useRebuildThemes } from './useClustersQuery';

const h = vi.hoisted(() => ({
  toast: { success: vi.fn(), error: vi.fn() },
  api: { listClusters: vi.fn(), clusterVaultRun: vi.fn() },
}));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));
vi.mock('@/lib/api', () => ({ default: h.api, VaultAPI: h.api }));

const api = h.api;
let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;

describe('useClustersQuery', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  });

  it('lists themes only when enabled', async () => {
    api.listClusters.mockResolvedValue({ ok: true, data: [{ id: 1, label: 'Fermentation' }] } as never);
    const { result, rerender } = renderHook(({ enabled }) => useClustersQuery(enabled), { wrapper, initialProps: { enabled: false } });
    expect(api.listClusters).not.toHaveBeenCalled();
    rerender({ enabled: true });
    await waitFor(() => expect(result.current.data).toEqual([{ id: 1, label: 'Fermentation' }]));
  });

  it('turns a refused list into an error', async () => {
    api.listClusters.mockResolvedValueOnce({ ok: false, error: 'no embeddings yet' } as never);
    const { result } = renderHook(() => useClustersQuery(true), { wrapper });
    await waitFor(() => expect(result.current.error?.message).toBe('no embeddings yet'));
  });

  describe('useRebuildThemes', () => {
    const rebuild = async () => {
      const { result } = renderHook(() => useRebuildThemes(), { wrapper });
      await act(async () => { result.current.mutate(); });
      return result;
    };

    it.each([
      [{ clusterCount: 0, noiseCount: 0 }, 'No themes found yet', undefined],
      [{ clusterCount: 1, noiseCount: 0 }, '1 theme', undefined],
      [{ clusterCount: 7, noiseCount: 1200 }, '7 themes', { message: `${(1200).toLocaleString()} documents didn't fit one.` }],
    ])('announces %j', async (run, title, options) => {
      vi.spyOn(client, 'invalidateQueries');
      api.clusterVaultRun.mockResolvedValueOnce({ ok: true, data: run } as never);
      await rebuild();
      await waitFor(() => expect(h.toast.success).toHaveBeenCalledWith(title, options));
      expect(client.invalidateQueries).toHaveBeenCalledWith({ queryKey: THEMES_QUERY_KEY });
    });

    it('reports a run that failed', async () => {
      api.clusterVaultRun.mockResolvedValueOnce({ ok: false, error: 'too few documents' } as never);
      await rebuild();
      await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith("Couldn't find themes", { message: 'too few documents' }));
    });
  });

  describe('useRebuildProgress', () => {
    it('reports the latest step while running and nothing once done', async () => {
      let emit!: (event: { payload: unknown }) => void;
      const unlisten = vi.fn();
      vi.mocked(listen).mockImplementationOnce(async (_name, handler) => { emit = handler as never; return unlisten; });
      const { result, rerender, unmount } = renderHook(({ running }) => useRebuildProgress(running), { initialProps: { running: true } });
      await waitFor(() => expect(listen).toHaveBeenCalledWith('corpus-shape://progress', expect.any(Function)));
      expect(result.current).toBeNull();
      act(() => emit({ payload: { phase: 'embedding', done: 3, total: 10 } }));
      expect(result.current).toEqual({ phase: 'embedding', done: 3, total: 10 });
      rerender({ running: false });
      expect(result.current).toBeNull();
      rerender({ running: true });
      expect(result.current).toBeNull();
      unmount();
      expect(unlisten).toHaveBeenCalled();
    });

    it('drops a subscription that lands after unmount', async () => {
      const subscribed = deferred<() => void>();
      const unlisten = vi.fn();
      vi.mocked(listen).mockReturnValueOnce(subscribed.promise as never);
      const { unmount } = renderHook(() => useRebuildProgress());
      unmount();
      await act(async () => { subscribed.resolve(unlisten); await subscribed.promise; });
      expect(unlisten).toHaveBeenCalled();
    });

    it('works without the event bridge', async () => {
      vi.mocked(listen).mockRejectedValueOnce(new Error('no bridge'));
      const { result } = renderHook(() => useRebuildProgress());
      await act(async () => {});
      expect(result.current).toBeNull();
    });
  });
});
