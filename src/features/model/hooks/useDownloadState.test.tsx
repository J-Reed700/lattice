import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider, notifyManager } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useDownloadStore } from '@/features/model/stores/downloadStore';
import type { DownloadStatus } from '@/types/downloads';

import { DOWNLOADS_QUERY_KEY } from './useDownloads';
import { useDownloadState } from './useDownloadState';

const api = vi.hoisted(() => ({ listDownloads: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: api, VaultAPI: api }));

// Rows as the backend lists them: states arrive in lowercase and are mapped on read.
const row = (id: string, overrides: Partial<Omit<DownloadStatus, 'state'>> & { state?: string } = {}): DownloadStatus => ({
  id,
  url: `https://example.org/${id}`,
  destination: `/models/${id}.gguf`,
  state: 'Pending',
  bytes_downloaded: 0,
  total_bytes: 100,
  bytes_per_second: 0,
  percentage: 0,
  eta_seconds: null,
  error_message: null,
  retry_count: 0,
  created_at: '2026-09-16T00:00:00.000Z',
  started_at: null,
  completed_at: null,
  model_name: id,
  model_id: null,
  ...overrides,
} as DownloadStatus);

function setup() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return { client, ...renderHook(() => useDownloadState(), { wrapper }) };
}

beforeEach(() => {
  api.listDownloads.mockReset().mockResolvedValue({ ok: true, data: [] });
  useDownloadStore.setState({ isDrawerOpen: false, listenerError: null });
  // Deliver cache updates to the hook synchronously so each act() sees them.
  notifyManager.setScheduler(callback => callback());
});
afterEach(() => {
  vi.useRealTimers();
  notifyManager.setScheduler(callback => setTimeout(callback, 0));
});

describe('useDownloadState', () => {
  it('loads persisted downloads and splits out the running ones', async () => {
    api.listDownloads.mockResolvedValue({
      ok: true,
      data: [
        row('herbs', { state: 'downloading', model_id: 'garden' }),
        row('spices', { state: 'paused' }),
        row('bread', { state: 'completed', model_id: 'garden' }),
        row('soup', { state: 'error' }),
      ],
    });
    const { result } = setup();
    expect(result.current.isLoading).toBe(true);
    await waitFor(() => expect(result.current.totalCount).toBe(4));

    expect(result.current.activeCount).toBe(2);
    expect(result.current.getActiveCount()).toBe(2);
    expect(result.current.getTotalCount()).toBe(4);
    expect(result.current.activeDownloads.map(d => d.id)).toEqual(['herbs', 'spices']);
    expect(result.current.getActiveDownloads()).toBe(result.current.activeDownloads);
    expect(result.current.getAllDownloads()).toBe(result.current.downloads);
    expect([...result.current.activeDownloadsSet]).toEqual(['garden:herbs.gguf', 'spices']);
    expect(result.current.getDownload('soup')?.state).toBe('Failed');
    expect(result.current.getDownloadsByModel('garden').map(d => d.id)).toEqual(['herbs', 'bread']);
    expect(result.current.getActiveDownloadForModel('garden')?.id).toBe('herbs');
    expect(result.current.hasActiveDownloadForModel('garden')).toBe(true);
    expect(result.current.getActiveDownloadForModel('orchard')).toBeNull();
    expect(result.current.hasActiveDownloadForModel('orchard')).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it('reports a failed listing as an error with no rows', async () => {
    api.listDownloads.mockResolvedValue({ ok: false, error: 'disk unavailable' });
    const { result } = setup();
    await waitFor(() => expect(result.current.error).toContain('disk unavailable'));
    expect(result.current.downloads).toEqual([]);
    expect(result.current.getLatestDownloadForModel('garden')).toBeNull();
  });

  it('picks the most recently started download for a model', async () => {
    api.listDownloads.mockResolvedValue({
      ok: true,
      data: [
        row('first', { model_id: 'garden', destination: '/m/garden/a.gguf', started_at: '2026-09-17T00:00:00.000Z', state: 'completed' }),
        row('second', { model_id: 'garden', destination: '/m/garden/b.gguf', created_at: '2026-09-18T00:00:00.000Z', state: 'failed' }),
        row('older', { model_id: 'garden', destination: '/m/garden/c.gguf', created_at: '2026-09-01T00:00:00.000Z', state: 'cancelled' }),
      ],
    });
    const { result } = setup();
    await waitFor(() => expect(result.current.totalCount).toBe(3));
    expect(result.current.getLatestDownloadForModel('garden')?.id).toBe('second');
  });

  it('applies progress, state and error updates only to rows it knows', async () => {
    const { result } = setup();
    await waitFor(() => expect(result.current.isLoading).toBe(false));

    act(() => result.current.setDownload('herbs', row('herbs', { state: 'Downloading' })));
    act(() => result.current.updateDownloadProgress('herbs', 40, 10, 40, 6));
    expect(result.current.getDownload('herbs')).toMatchObject({ bytes_downloaded: 40, bytes_per_second: 10, percentage: 40, eta_seconds: 6 });

    const before = result.current.downloadsMap;
    act(() => result.current.updateDownloadProgress('ghost', 1, 1, null, null));
    act(() => result.current.updateDownloadState('ghost', 'Completed'));
    act(() => result.current.setDownloadError('ghost', 'lost'));
    expect(result.current.downloadsMap).toBe(before);

    act(() => result.current.updateDownloadState('herbs', 'Paused'));
    expect(result.current.getDownload('herbs')).toMatchObject({ state: 'Paused', completed_at: null });
    act(() => result.current.updateDownloadState('herbs', 'Completed'));
    const finishedAt = result.current.getDownload('herbs')?.completed_at;
    expect(finishedAt).toEqual(expect.any(String));
    // A second terminal state keeps the first completion time.
    act(() => result.current.updateDownloadState('herbs', 'Failed'));
    expect(result.current.getDownload('herbs')?.completed_at).toBe(finishedAt);

    act(() => result.current.setDownload('spices', row('spices', { state: 'Downloading' })));
    act(() => result.current.setDownloadError('spices', 'checksum mismatch'));
    expect(result.current.getDownload('spices')).toMatchObject({ state: 'Failed', error_message: 'checksum mismatch', completed_at: expect.any(String) });
    act(() => result.current.setDownload('soup', row('soup', { completed_at: '2026-09-20T00:00:00.000Z' })));
    act(() => result.current.setDownloadError('soup', 'again'));
    expect(result.current.getDownload('soup')?.completed_at).toBe('2026-09-20T00:00:00.000Z');

    act(() => result.current.removeDownload('herbs'));
    expect(result.current.getDownload('herbs')).toBeUndefined();
    act(() => result.current.clearDownloads());
    expect(result.current.totalCount).toBe(0);
  });

  it('refetches the persisted list when old rows are cleaned up', async () => {
    const { result } = setup();
    await waitFor(() => expect(api.listDownloads).toHaveBeenCalledTimes(1));
    act(() => result.current.cleanupOldDownloads());
    await waitFor(() => expect(api.listDownloads).toHaveBeenCalledTimes(2));
  });

  it('polls only while a download is pending or running', async () => {
    vi.useFakeTimers();
    api.listDownloads.mockResolvedValue({ ok: true, data: [row('herbs', { state: 'downloading' })] });
    const { result, client } = setup();
    await act(async () => { await vi.advanceTimersByTimeAsync(0); });
    expect(result.current.activeCount).toBe(1);
    expect(api.listDownloads).toHaveBeenCalledTimes(1);

    api.listDownloads.mockResolvedValue({ ok: true, data: [row('herbs', { state: 'completed' })] });
    await act(async () => { await vi.advanceTimersByTimeAsync(5_000); });
    expect(api.listDownloads).toHaveBeenCalledTimes(2);
    expect(result.current.activeCount).toBe(0);

    await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
    expect(api.listDownloads).toHaveBeenCalledTimes(2);
    expect(client.getQueryData<Map<string, DownloadStatus>>(DOWNLOADS_QUERY_KEY)?.get('herbs')?.state).toBe('Completed');
  });

  it('exposes the drawer and listener state from the downloads store', async () => {
    const { result } = setup();
    act(() => result.current.openDrawer());
    expect(result.current.isDrawerOpen).toBe(true);
    act(() => result.current.toggleDrawer());
    expect(result.current.isDrawerOpen).toBe(false);
    act(() => result.current.toggleDrawer());
    act(() => result.current.closeDrawer());
    expect(result.current.isDrawerOpen).toBe(false);
    act(() => result.current.setListenerError('event stream closed'));
    expect(result.current.listenerError).toBe('event stream closed');
    await waitFor(() => expect(result.current.isLoading).toBe(false));
  });
});
