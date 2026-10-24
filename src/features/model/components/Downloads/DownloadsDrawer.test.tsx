import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { DOWNLOADS_QUERY_KEY } from '@/features/model/hooks/useDownloads';
import { useDownloadStore } from '@/features/model/stores/downloadStore';
import { useToastStore } from '@/stores/toastStore';
import { deferred } from '@/tests/deferred';
import type { DownloadState, DownloadStatus } from '@/types/downloads';

import { DownloadsDrawer } from './DownloadsDrawer';
import { DrawerTrigger } from './DrawerTrigger';

const actions = vi.hoisted(() => ({
  pauseDownload: vi.fn(),
  resumeDownload: vi.fn(),
  cancelDownload: vi.fn(),
  retryDownload: vi.fn(),
  removeDownload: vi.fn(),
  clearCompletedDownloads: vi.fn(),
}));
const api = vi.hoisted(() => ({ listDownloads: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: api, VaultAPI: api }));
vi.mock('@/features/model/hooks/useDownloads', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/features/model/hooks/useDownloads')>()),
  useDownloadActions: () => actions,
}));

const row = (id: string, state: DownloadState, overrides: Partial<DownloadStatus> = {}): DownloadStatus => ({
  id, url: `https://example.org/${id}`, destination: `/models/${id}.gguf`, state, bytes_downloaded: 512, total_bytes: 1024,
  bytes_per_second: 2048, percentage: null, eta_seconds: 90, error_message: null, retry_count: 0,
  created_at: '2026-10-01T00:00:00Z', started_at: null, completed_at: null, model_name: '', model_id: null, ...overrides,
});

function renderDownloads(entries: Array<[string, DownloadStatus]>, children: ReactNode = <><DownloadsDrawer /><DrawerTrigger /></>) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
  client.setQueryData(DOWNLOADS_QUERY_KEY, new Map(entries));
  api.listDownloads.mockResolvedValue({ ok: true, data: entries.map(([, value]) => value) });
  render(<QueryClientProvider client={client}>{children}</QueryClientProvider>);
  return client;
}

const toasts = () => useToastStore.getState().toasts.map(({ title, message }) => ({ title, message }));

beforeEach(() => {
  for (const fn of Object.values(actions)) fn.mockReset().mockResolvedValue(undefined);
  useDownloadStore.setState({ isDrawerOpen: false, listenerError: null });
  useToastStore.setState({ toasts: [] });
});

describe('DrawerTrigger', () => {
  it('shows aggregate progress of active downloads and opens the drawer', async () => {
    const user = userEvent.setup();
    renderDownloads([
      ['a', row('a', 'Downloading', { bytes_downloaded: 300, total_bytes: 1000 })],
      ['b', row('b', 'Pending', { bytes_downloaded: 0, total_bytes: null })],
      ['c', row('c', 'Completed')],
    ]);
    const trigger = screen.getByRole('button', { name: 'Show downloads (2 active)' });
    expect(trigger).toHaveTextContent('2 downloads');
    expect(trigger.querySelector('[style]')).toHaveStyle({ width: '30%' });
    await user.click(trigger);
    expect(await screen.findByRole('dialog', { name: 'Downloads' })).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByRole('button', { name: /Show downloads/ })).not.toBeInTheDocument());
  });

  it('pulses when no active download has a known size, and hides with nothing active', () => {
    renderDownloads([['a', row('a', 'Paused', { total_bytes: null })]]);
    const trigger = screen.getByRole('button', { name: 'Show downloads (1 active)' });
    expect(trigger).toHaveTextContent('1 download');
    expect(trigger.querySelector('[style]')).toBeNull();
  });

  it('stays hidden when nothing is downloading', () => {
    renderDownloads([['a', row('a', 'Completed')]]);
    expect(screen.queryByRole('button', { name: /Show downloads/ })).not.toBeInTheDocument();
  });
});

describe('DownloadsDrawer', () => {
  it('says when there are no downloads and closes', async () => {
    const user = userEvent.setup();
    useDownloadStore.setState({ isDrawerOpen: true });
    renderDownloads([]);
    expect(screen.getByText('No downloads')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Clear completed' })).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Close downloads' }));
    expect(useDownloadStore.getState().isDrawerOpen).toBe(false);
  });

  it('shows and dismisses a listener error', async () => {
    const user = userEvent.setup();
    useDownloadStore.setState({ isDrawerOpen: true, listenerError: 'Download events stopped' });
    renderDownloads([]);
    expect(screen.getByText('Download events stopped')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Dismiss download error' }));
    expect(screen.queryByText('Download events stopped')).not.toBeInTheDocument();
  });

  it('offers the actions each state allows, keyed by the store key', async () => {
    const user = userEvent.setup();
    useDownloadStore.setState({ isDrawerOpen: true });
    renderDownloads([
      ['k-run', row('run', 'Downloading', { model_name: 'Fern atlas' })],
      ['k-paused', row('paused', 'Paused')],
      ['k-failed', row('failed', 'Failed', { error_message: 'Connection reset' })],
      ['k-cancelled', row('cancelled', 'Cancelled', { bytes_downloaded: 0 })],
      ['k-done', row('done', 'Completed')],
    ]);
    const drawer = screen.getByRole('dialog', { name: 'Downloads' });
    expect(within(drawer).getByText('Fern atlas')).toBeInTheDocument();
    expect(within(drawer).getByText('1m 30s left')).toBeInTheDocument();
    expect(within(drawer).getByText('Connection reset')).toBeInTheDocument();
    expect(within(drawer).getByRole('progressbar', { name: 'Fern atlas download progress' })).toHaveAttribute('aria-valuenow', '50');
    expect(within(drawer).queryByRole('progressbar', { name: 'done.gguf download progress' })).not.toBeInTheDocument();

    expect(within(drawer).queryByRole('button', { name: 'Resume Fern atlas' })).not.toBeInTheDocument();
    await user.click(within(drawer).getByRole('button', { name: 'Pause Fern atlas' }));
    expect(actions.pauseDownload).toHaveBeenCalledWith('k-run');
    await user.click(within(drawer).getByRole('button', { name: 'Cancel Fern atlas' }));
    expect(actions.cancelDownload).toHaveBeenCalledWith('k-run');
    await user.click(within(drawer).getByRole('button', { name: 'Resume paused.gguf' }));
    expect(actions.resumeDownload).toHaveBeenCalledWith('k-paused');
    // A failed download with bytes on disk can resume or start over.
    await user.click(within(drawer).getByRole('button', { name: 'Resume failed.gguf' }));
    await user.click(within(drawer).getByRole('button', { name: 'Retry failed.gguf' }));
    expect(actions.retryDownload).toHaveBeenCalledWith('k-failed');
    expect(within(drawer).queryByRole('button', { name: 'Resume cancelled.gguf' })).not.toBeInTheDocument();
    await user.click(within(drawer).getByRole('button', { name: 'Remove done.gguf from list' }));
    expect(actions.removeDownload).toHaveBeenCalledWith('k-done');
  });

  it('disables a row while its action runs and reports a failure', async () => {
    const user = userEvent.setup();
    const pending = deferred<void>();
    actions.pauseDownload.mockReturnValue(pending.promise);
    useDownloadStore.setState({ isDrawerOpen: true });
    renderDownloads([['k-run', row('run', 'Downloading')]]);
    await user.click(screen.getByRole('button', { name: 'Pause run.gguf' }));
    expect(screen.getByRole('button', { name: 'Cancel run.gguf' })).toBeDisabled();
    await act(async () => { pending.reject(new Error('backend busy')); });
    await waitFor(() => expect(screen.getByRole('button', { name: 'Cancel run.gguf' })).toBeEnabled());
    expect(toasts()).toEqual([{ title: 'Could not pause download', message: 'backend busy' }]);
  });

  it('clears completed downloads and reports a failure', async () => {
    const user = userEvent.setup();
    useDownloadStore.setState({ isDrawerOpen: true });
    renderDownloads([['k-done', row('done', 'Completed')]]);
    await user.click(screen.getByRole('button', { name: 'Clear completed' }));
    expect(actions.clearCompletedDownloads).toHaveBeenCalled();
    actions.clearCompletedDownloads.mockRejectedValue(new Error('locked'));
    await user.click(screen.getByRole('button', { name: 'Clear completed' }));
    await waitFor(() => expect(toasts()).toEqual([{ title: 'Could not clear completed downloads', message: 'locked' }]));
  });

  it('groups a multi-file model and expands to its files', async () => {
    const user = userEvent.setup();
    useDownloadStore.setState({ isDrawerOpen: true });
    renderDownloads([
      ['bird/config.json', row('row-1', 'Completed', { model_id: 'bird', model_name: 'Bird calls', destination: '/models/bird/config.json', bytes_downloaded: 1024 })],
      ['bird/weights', row('row-2', 'Downloading', { model_id: 'bird', model_name: 'Bird calls', destination: '/models/bird/1_Pooling/weights.bin', bytes_per_second: 4096 })],
    ]);
    const group = screen.getByRole('button', { name: /Bird calls/ });
    expect(group).toHaveAttribute('aria-expanded', 'false');
    expect(group).toHaveTextContent('1/2 files');
    expect(screen.getByRole('progressbar', { name: 'Bird calls download progress' })).toBeInTheDocument();
    await user.click(group);
    expect(group).toHaveAttribute('aria-expanded', 'true');
    await user.click(screen.getByRole('button', { name: 'Pause 1_Pooling/weights.bin' }));
    expect(actions.pauseDownload).toHaveBeenCalledWith('bird/weights');
    expect(screen.getByRole('button', { name: 'Remove config.json from list' })).toBeInTheDocument();
  });

  it('shows a failed group’s message and no progress for a finished group', () => {
    useDownloadStore.setState({ isDrawerOpen: true });
    renderDownloads([
      ['m/a', row('a', 'Failed', { model_id: 'm', model_name: 'Moss', error_message: 'Disk full' })],
      ['m/b', row('b', 'Completed', { model_id: 'm', model_name: 'Moss' })],
      ['n/a', row('na', 'Completed', { model_id: 'n', model_name: 'Lichen' })],
      ['n/b', row('nb', 'Completed', { model_id: 'n', model_name: 'Lichen' })],
    ]);
    expect(screen.getByText('Disk full')).toBeInTheDocument();
    expect(screen.queryByRole('progressbar', { name: 'Lichen download progress' })).not.toBeInTheDocument();
  });
});
