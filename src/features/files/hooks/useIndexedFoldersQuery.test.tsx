import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SETTINGS_QUERY_KEY } from '@/features/settings/hooks/useSettingsQuery';

import { INDEXED_FOLDERS_QUERY_KEY, useIndexedFoldersQuery, useSetWatchFolderEnabledMutation } from './useIndexedFoldersQuery';

const h = vi.hoisted(() => ({ api: { getIndexedFolders: vi.fn(), addWatchFolder: vi.fn(), removeWatchFolder: vi.fn() } }));
vi.mock('@/lib/api', () => ({ default: h.api, VaultAPI: h.api }));

const api = h.api;
let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;

describe('useIndexedFoldersQuery', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  });

  it('lists indexed folders', async () => {
    api.getIndexedFolders.mockResolvedValueOnce({ ok: true, data: [{ path: '/notes/history' }] } as never);
    const { result } = renderHook(() => useIndexedFoldersQuery(), { wrapper });
    await waitFor(() => expect(result.current.data).toEqual([{ path: '/notes/history' }]));
  });

  it('turns a refused list into an error', async () => {
    api.getIndexedFolders.mockResolvedValueOnce({ ok: false, error: 'db locked' } as never);
    const { result } = renderHook(() => useIndexedFoldersQuery(), { wrapper });
    await waitFor(() => expect(result.current.error?.message).toBe('db locked'));
  });

  describe('watching a folder', () => {
    const run = async (args: { path: string; enabled: boolean }) => {
      const { result } = renderHook(() => useSetWatchFolderEnabledMutation(), { wrapper });
      let error: unknown;
      await act(async () => { try { await result.current.mutateAsync(args); } catch (caught) { error = caught; } });
      return error as Error | undefined;
    };

    it('adds or removes the watch and refreshes folders and settings', async () => {
      vi.spyOn(client, 'invalidateQueries');
      api.addWatchFolder.mockResolvedValueOnce({ ok: true, data: undefined } as never);
      api.removeWatchFolder.mockResolvedValueOnce({ ok: true, data: undefined } as never);
      expect(await run({ path: '/notes/history', enabled: true })).toBeUndefined();
      expect(api.addWatchFolder).toHaveBeenCalledWith('/notes/history');
      expect(await run({ path: '/notes/history', enabled: false })).toBeUndefined();
      expect(api.removeWatchFolder).toHaveBeenCalledWith('/notes/history');
      expect(client.invalidateQueries).toHaveBeenCalledWith({ queryKey: INDEXED_FOLDERS_QUERY_KEY });
      expect(client.invalidateQueries).toHaveBeenCalledWith({ queryKey: SETTINGS_QUERY_KEY });
    });

    it('throws a refused change', async () => {
      api.addWatchFolder.mockResolvedValueOnce({ ok: false, error: 'folder missing' } as never);
      expect((await run({ path: '/gone', enabled: true }))?.message).toBe('folder missing');
    });
  });
});
