import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useDownloadStore } from '@/features/model/stores/downloadStore';
import type { DownloadStatus } from '@/types/downloads';

import { DOWNLOADS_QUERY_KEY, toDownloadState, useDownloadActions, useDownloadsListener } from './useDownloads';

const mocks = vi.hoisted(() => ({
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
  unlisten: vi.fn(),
  listen: vi.fn(),
  api: {
    listDownloads: vi.fn(),
    pauseDownload: vi.fn(),
    resumeDownload: vi.fn(),
    cancelDownload: vi.fn(),
    retryDownload: vi.fn(),
    removeDownload: vi.fn(),
    clearCompletedDownloads: vi.fn(),
    startVisionProjectorDownload: vi.fn(),
  },
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: mocks.listen }));
vi.mock('@/lib/api', () => ({ default: mocks.api, VaultAPI: mocks.api }));

const row = (id: string, overrides: Partial<DownloadStatus> = {}): DownloadStatus => ({
  id, url: `https://example.org/${id}`, destination: `/models/${id}.gguf`, state: 'Downloading', bytes_downloaded: 10,
  total_bytes: 100, bytes_per_second: 5, percentage: 10, eta_seconds: 18, error_message: null, retry_count: 1,
  created_at: '2026-10-01T00:00:00.000Z', started_at: '2026-10-01T00:00:01.000Z', completed_at: null,
  model_name: 'Herbarium', model_id: null, ...overrides,
});
const single = (id: string, status: string, overrides: Record<string, unknown> = {}) => ({
  kind: 'single', id, filename: `${id}.gguf`, bytesDownloaded: 50, totalBytes: 100, bytesPerSecond: 20,
  percentage: 50, etaSeconds: 3, status, ...overrides,
});

let client: QueryClient;
const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
const cached = () => client.getQueryData<Map<string, DownloadStatus>>(DOWNLOADS_QUERY_KEY)!;
const emit = (name: string, payload: unknown) => act(() => { mocks.handlers.get(name)!({ payload }); });

beforeEach(() => {
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  mocks.handlers.clear();
  mocks.unlisten.mockReset();
  mocks.listen.mockReset().mockImplementation(async (name: string, handler: (event: { payload: unknown }) => void) => {
    mocks.handlers.set(name, handler);
    return mocks.unlisten;
  });
  for (const fn of Object.values(mocks.api)) fn.mockReset().mockResolvedValue({ ok: true, data: null });
  mocks.api.listDownloads.mockResolvedValue({ ok: true, data: [row('known', { model_id: null })] });
  useDownloadStore.setState({ listenerError: null });
  vi.spyOn(console, 'error').mockImplementation(() => {});
});
afterEach(() => vi.restoreAllMocks());

async function listen() {
  const view = renderHook(() => useDownloadsListener(), { wrapper });
  await waitFor(() => expect(cached()?.has('known')).toBe(true));
  return view;
}

describe('useDownloadsListener', () => {
  it('merges progress into the row it knows, keeping its identity', async () => {
    await listen();
    emit('download:progress', single('known', 'downloading'));
    expect(cached().get('known')).toMatchObject({
      id: 'known', url: 'https://example.org/known', state: 'Downloading', bytes_downloaded: 50, percentage: 50,
      retry_count: 1, started_at: '2026-10-01T00:00:01.000Z', completed_at: null, model_name: 'Herbarium',
    });
  });

  it('adds a download it has not seen and stamps a finished one', async () => {
    await listen();
    emit('download:progress', single('fresh', 'completed', { totalBytes: null, percentage: null }));
    expect(cached().get('fresh')).toMatchObject({
      url: 'fresh.gguf', destination: 'fresh.gguf', state: 'Completed', total_bytes: null, model_name: 'fresh.gguf',
      completed_at: expect.any(String), retry_count: 0,
    });
  });

  it('reports an event it cannot read', async () => {
    await listen();
    emit('download:progress', { kind: 'single', id: 7 });
    expect(useDownloadStore.getState().listenerError).toBe('Invalid download event received from backend');
    emit('download:failed', { id: 3 });
    expect(console.error).toHaveBeenCalledWith('[useDownloadsListener] Failed event validation error:', expect.anything());
  });

  it('marks a failed download and says why', async () => {
    await listen();
    emit('download:failed', { id: 'known', error: 'checksum mismatch' });
    expect(useDownloadStore.getState().listenerError).toBe('Download failed: checksum mismatch');
    expect(cached().get('known')).toMatchObject({ state: 'Failed', error_message: 'checksum mismatch', completed_at: expect.any(String) });
    const before = cached();
    emit('download:failed', { id: 'unknown', error: 'gone' });
    expect(cached()).toBe(before);
  });

  it('stops listening on unmount and ignores events after it', async () => {
    const { unmount } = await listen();
    const progress = mocks.handlers.get('download:progress')!;
    unmount();
    expect(mocks.unlisten).toHaveBeenCalledTimes(2);
    act(() => progress({ payload: single('known', 'completed') }));
    expect(cached().get('known')?.state).toBe('Downloading');
  });

  it('drops listeners that attach after unmount', async () => {
    let attach!: (unlisten: () => void) => void;
    mocks.listen.mockImplementationOnce(() => new Promise((resolve) => { attach = resolve; }));
    const { unmount } = renderHook(() => useDownloadsListener(), { wrapper });
    unmount();
    await act(async () => { attach(mocks.unlisten); });
    await waitFor(() => expect(mocks.unlisten).toHaveBeenCalledTimes(2));
    expect(mocks.api.listDownloads).not.toHaveBeenCalled();
  });

  it('reports a listener that could not be set up, and still loads the list', async () => {
    mocks.listen.mockRejectedValueOnce(new Error('event bus closed'));
    renderHook(() => useDownloadsListener(), { wrapper });
    await waitFor(() => expect(useDownloadStore.getState().listenerError).toBe('event bus closed'));
    await waitFor(() => expect(cached()?.has('known')).toBe(true));
  });
});

describe('useDownloadActions', () => {
  function actions() {
    const weights = row('session-7', { model_id: 'garden', destination: '/models/garden/weights.gguf' });
    // The backend list keeps the row, so its store key still resolves after each re-read.
    mocks.api.listDownloads.mockResolvedValue({ ok: true, data: [weights] });
    client.setQueryData(DOWNLOADS_QUERY_KEY, new Map([['garden:weights.gguf', weights]]));
    return renderHook(() => useDownloadActions(), { wrapper }).result;
  }

  it('acts on the backend session behind a store key, then re-reads the list', async () => {
    const result = actions();
    expect(result.current.downloads?.size).toBe(1);
    await act(() => result.current.cancelDownload('garden:weights.gguf'));
    expect(mocks.api.cancelDownload).toHaveBeenCalledWith('session-7');
    await act(() => result.current.retryDownload('loose-id'));
    expect(mocks.api.retryDownload).toHaveBeenCalledWith('loose-id');
    await act(() => result.current.removeDownload('garden:weights.gguf'));
    expect(mocks.api.removeDownload).toHaveBeenCalledWith('session-7');
    await act(() => result.current.fetchAllDownloads());
    expect(mocks.api.listDownloads).toHaveBeenCalledTimes(4);
  });

  it('treats removing a session the backend lost as done, and reports other failures', async () => {
    const result = actions();
    mocks.api.removeDownload.mockResolvedValueOnce({ ok: false, error: 'gone', details: { code: 'NOT_FOUND', message: 'gone' } });
    await expect(result.current.removeDownload('x')).resolves.toBeUndefined();
    mocks.api.removeDownload.mockResolvedValueOnce({ ok: false, error: 'locked', details: { code: 'DATABASE_ERROR', message: 'locked' } });
    await expect(result.current.removeDownload('x')).rejects.toThrow('Failed to remove download: locked');
  });

  it('clears finished downloads and returns how many', async () => {
    const result = actions();
    mocks.api.clearCompletedDownloads.mockResolvedValueOnce({ ok: true, data: 3 });
    await expect(result.current.clearCompletedDownloads()).resolves.toBe(3);
    mocks.api.clearCompletedDownloads.mockResolvedValueOnce({ ok: false, error: 'busy', details: { code: 'DATABASE_ERROR', message: 'busy' } });
    await expect(result.current.clearCompletedDownloads()).rejects.toThrow('Failed to clear completed downloads: busy');
  });

  it('starts a vision projector download, or reports none was needed', async () => {
    const result = actions();
    mocks.api.startVisionProjectorDownload.mockResolvedValueOnce({ ok: true, data: { downloadId: 'mmproj-1' } });
    await expect(result.current.startVisionProjectorDownload('garden')).resolves.toBe('mmproj-1');
    expect(mocks.api.listDownloads).toHaveBeenCalledTimes(1);
    mocks.api.startVisionProjectorDownload.mockResolvedValueOnce({ ok: true, data: { downloadId: null } });
    await expect(result.current.startVisionProjectorDownload('garden')).resolves.toBeNull();
    expect(mocks.api.listDownloads).toHaveBeenCalledTimes(1);
    mocks.api.startVisionProjectorDownload.mockResolvedValueOnce({ ok: false, error: 'no projector', details: { code: 'NOT_FOUND', message: 'no projector' } });
    await expect(result.current.startVisionProjectorDownload('garden')).rejects.toThrow('no projector');
  });
});

describe('toDownloadState', () => {
  it.each([
    ['PENDING', 'Pending'], ['downloading', 'Downloading'], ['paused', 'Paused'], ['completed', 'Completed'],
    ['error', 'Failed'], ['failed', 'Failed'], ['cancelled', 'Cancelled'], ['mystery', 'Pending'],
  ])('reads %s as %s', (status, state) => {
    expect(toDownloadState(status)).toBe(state);
  });
});
