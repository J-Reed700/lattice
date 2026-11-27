import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiProvider } from '@/contexts/ApiContext';
import { useDownloadStore } from '@/features/model/stores/downloadStore';
import type { RerankerStatusDto } from '@/lib/bindings';
import { useToastStore } from '@/stores/toastStore';
import { createTestApi } from '@/tests/apiClient';
import { deferred } from '@/tests/deferred';

import { RerankerRow } from './RerankerRow';

const PATH = '/models/reranker/qwen3-reranker-0.6b.gguf';
const status = (installed: boolean): RerankerStatusDto => ({
  installed, modelName: 'Qwen/Qwen3-Reranker-0.6B', downloadBytes: 639_153_184, path: PATH,
});

// Rows as the backend lists them: states arrive in lowercase.
const listed = (state: string, overrides: Record<string, unknown> = {}) => ({
  id: 'dl-1', url: 'https://example.org/r.gguf', destination: PATH, state, bytes_downloaded: 40, total_bytes: 100,
  bytes_per_second: 1, percentage: 40, eta_seconds: null, error_message: null, retry_count: 0,
  created_at: '2026-10-01T00:00:00Z', started_at: null, completed_at: null, model_name: 'Qwen', model_id: null,
  ...overrides,
});

interface Setup {
  rerankerStatus?: () => Promise<unknown>;
  downloads?: () => Promise<unknown>;
  downloadReranker?: () => Promise<unknown>;
  removeReranker?: () => Promise<unknown>;
}

function makeApi({ rerankerStatus, downloads, downloadReranker, removeReranker }: Setup) {
  return createTestApi({
    getRerankerStatus: vi.fn(rerankerStatus ?? (async () => ({ ok: true as const, data: status(false) }))) as never,
    listDownloads: vi.fn(downloads ?? (async () => ({ ok: true as const, data: [] }))) as never,
    downloadReranker: vi.fn(downloadReranker ?? (async () => ({ ok: true as const, data: status(false) }))) as never,
    removeReranker: vi.fn(removeReranker ?? (async () => ({ ok: true as const, data: status(false) }))) as never,
    resumeDownload: vi.fn(async () => ({ ok: true as const, data: undefined })) as never,
  });
}

const tree = (api: ReturnType<typeof makeApi>, client: QueryClient) => (
  <ApiProvider client={api}>
    <QueryClientProvider client={client}>
      <RerankerRow />
    </QueryClientProvider>
  </ApiProvider>
);

function setup(options: Setup = {}) {
  const api = makeApi(options);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const view = render(tree(api, client));
  return { api, client, view, user: userEvent.setup() };
}

const downloadButton = () => screen.findByRole('button', { name: /^Download \(639 MB\)$/ });

beforeEach(() => {
  useDownloadStore.setState({ isDrawerOpen: false });
  useToastStore.setState({ toasts: [] });
});
afterEach(() => vi.useRealTimers());

describe('RerankerRow', () => {
  it('shows the hint and a Remove button once the reranker is installed', async () => {
    setup({ rerankerStatus: async () => ({ ok: true, data: status(true) }) });
    expect(await screen.findByText(/A cross-encoder rescores the shortlist/)).toBeInTheDocument();
    expect(await screen.findByText('Downloaded')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Remove the reranker' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Download/ })).toBeNull();
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('removes the reranker once confirmed, and offers the download again', async () => {
    let installed = true;
    const { api, user } = setup({
      rerankerStatus: async () => ({ ok: true, data: status(installed) }),
      removeReranker: async () => {
        installed = false;
        return { ok: true, data: status(false) };
      },
    });
    await user.click(await screen.findByRole('button', { name: 'Remove the reranker' }));
    expect(await screen.findByText(/This frees 639 MB\./)).toBeInTheDocument();
    await user.click(await screen.findByRole('button', { name: /^Remove\s*⌘/ }));

    expect(await downloadButton()).toBeInTheDocument();
    expect(api.removeReranker).toHaveBeenCalledTimes(1);
    expect(useToastStore.getState().toasts.map((t) => t.title)).toContain('Removed the reranker');
  });

  it('never reads Finishing… after removing a model that just finished downloading', async () => {
    let installed = true;
    let sessions = [listed('completed', { completed_at: new Date(Date.now() - 5_000).toISOString(), percentage: 100 })];
    const refetch = deferred<unknown>();
    let holdList = false;
    const { user } = setup({
      rerankerStatus: async () => ({ ok: true, data: status(installed) }),
      downloads: async () => {
        if (holdList) return refetch.promise;
        return { ok: true, data: sessions };
      },
      removeReranker: async () => {
        installed = false;
        sessions = [];
        holdList = true;
        return { ok: true, data: status(false) };
      },
    });
    await user.click(await screen.findByRole('button', { name: 'Remove the reranker' }));
    await user.click(await screen.findByRole('button', { name: /^Remove\s*⌘/ }));

    // The downloads refetch is pending: the status must not have flipped yet.
    await waitFor(() => expect(holdList).toBe(true));
    await act(async () => { await Promise.resolve(); });
    expect(screen.queryByText('Finishing…')).toBeNull();

    await act(async () => refetch.resolve({ ok: true, data: [] }));
    expect(await downloadButton()).toBeInTheDocument();
    expect(screen.queryByText('Finishing…')).toBeNull();
  });

  it('stays installed and says why when the removal fails', async () => {
    const { user } = setup({
      rerankerStatus: async () => ({ ok: true, data: status(true) }),
      removeReranker: async () => ({ ok: false, error: 'The download is still running.' }),
    });
    await user.click(await screen.findByRole('button', { name: 'Remove the reranker' }));
    await user.click(await screen.findByRole('button', { name: /^Remove\s*⌘/ }));

    await waitFor(() => expect(useToastStore.getState().toasts.map((t) => t.title)).toContain("Couldn't remove the reranker"));
    expect(useToastStore.getState().toasts.some((t) => t.message === 'The download is still running.')).toBe(true);
    expect(screen.getByText('Downloaded')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Remove the reranker' })).toBeEnabled();
  });

  it('shows clear status and retry instead of a blank row when status is unavailable', async () => {
    const { api, user } = setup({
      rerankerStatus: async () => ({ ok: false, error: 'backend unavailable' }),
    });
    expect(await screen.findByText('Not configured')).toBeInTheDocument();
    expect(screen.getByText(/results keep their first-stage search order/i)).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Retry' }));
    await waitFor(() => expect(api.getRerankerStatus).toHaveBeenCalledTimes(2));
  });

  it('offers the download when idle, and starts it', async () => {
    const { api, user } = setup();
    const button = await downloadButton();
    await waitFor(() => expect(button).toBeEnabled());
    expect(screen.getByText(/not being reranked/)).toBeInTheDocument();

    await user.click(button);
    expect(api.downloadReranker).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(useToastStore.getState().toasts.map((t) => t.title)).toContain('Download started'));
  });

  it('holds the button until the download list has loaded', async () => {
    const list = deferred<unknown>();
    setup({ downloads: () => list.promise });
    const button = await downloadButton();
    expect(button).toBeDisabled();
    await act(async () => list.resolve({ ok: true, data: [] }));
    await waitFor(() => expect(button).toBeEnabled());
  });

  it.each([
    ['downloading', 'Downloading… 40%'],
    ['pending', 'Queued'],
  ])('follows a %s download from the shared list', async (state, text) => {
    setup({ downloads: async () => ({ ok: true, data: [listed(state)] }) });
    expect(await screen.findByRole('status', { name: 'Downloading the reranker' })).toHaveTextContent(text);
    expect(screen.getByText('Reranking turns on when the download finishes.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^Download/ })).toBeNull();
  });

  it('resumes a paused download', async () => {
    const { api, user } = setup({ downloads: async () => ({ ok: true, data: [listed('paused')] }) });
    await user.click(await screen.findByRole('button', { name: 'Resume' }));
    expect(api.resumeDownload).toHaveBeenCalledWith('dl-1');
  });

  it('opens the Downloads drawer', async () => {
    const { user } = setup({ downloads: async () => ({ ok: true, data: [listed('downloading')] }) });
    await user.click(await screen.findByRole('button', { name: 'Show in Downloads' }));
    expect(useDownloadStore.getState().isDrawerOpen).toBe(true);
  });

  it('says why the last attempt failed, and offers the download again', async () => {
    setup({ downloads: async () => ({ ok: true, data: [listed('error', { error_message: 'Couldn’t reach the download host.' })] }) });
    expect(await screen.findByText("Couldn't download the reranker: Couldn’t reach the download host.")).toBeInTheDocument();
    expect(await downloadButton()).toBeInTheDocument();
  });

  it('keeps showing Starting… after the row is remounted mid-call', async () => {
    const call = deferred<unknown>();
    const { api, client, view, user } = setup({ downloadReranker: () => call.promise });
    const button = await downloadButton();
    await waitFor(() => expect(button).toBeEnabled());
    await user.click(button);
    expect(await screen.findByRole('button', { name: 'Starting…' })).toBeDisabled();

    view.unmount();
    render(tree(api, client));
    expect(await screen.findByRole('button', { name: 'Starting…' })).toBeDisabled();

    await act(async () => call.resolve({ ok: true, data: status(false) }));
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Starting…' })).toBeNull());
  });

  it('toasts when the download cannot start', async () => {
    const { user } = setup({ downloadReranker: async () => ({ ok: false, error: 'disk full' }) });
    const button = await downloadButton();
    await waitFor(() => expect(button).toBeEnabled());
    await user.click(button);
    await waitFor(() =>
      expect(useToastStore.getState().toasts).toEqual(
        expect.arrayContaining([expect.objectContaining({ title: "Couldn't start the download", message: 'disk full' })]),
      ),
    );
  });

  it('says Already downloaded when the command finds the file on disk', async () => {
    const { user } = setup({ downloadReranker: async () => ({ ok: true, data: status(true) }) });
    const button = await downloadButton();
    await waitFor(() => expect(button).toBeEnabled());
    await user.click(button);
    await waitFor(() => expect(useToastStore.getState().toasts.map((t) => t.title)).toContain('Already downloaded'));
    expect(await screen.findByText(/A cross-encoder rescores the shortlist/)).toBeInTheDocument();
  });

  describe('finishing', () => {
    const NOW = Date.parse('2026-10-01T01:00:00Z');
    const finished = (secondsAgo: number) =>
      listed('completed', { percentage: 100, completed_at: new Date(NOW - secondsAgo * 1000).toISOString() });

    beforeEach(() => {
      vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'setInterval', 'clearInterval', 'Date'] });
      vi.setSystemTime(NOW);
    });

    it('re-reads the status every second, then gives the button back when the window ends', async () => {
      const { api } = setup({ downloads: async () => ({ ok: true, data: [finished(58)] }) });
      await act(async () => { await vi.advanceTimersByTimeAsync(0); });
      expect(screen.getByRole('status', { name: 'Downloading the reranker' })).toHaveTextContent('Finishing…');
      expect(screen.getByText('Reranking turns on when the download finishes.')).toBeInTheDocument();

      const before = vi.mocked(api.getRerankerStatus).mock.calls.length;
      await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
      expect(vi.mocked(api.getRerankerStatus).mock.calls.length).toBeGreaterThan(before);

      await act(async () => { await vi.advanceTimersByTimeAsync(1500); });
      expect(screen.queryByRole('status')).toBeNull();
      expect(screen.getByRole('button', { name: /^Download \(639 MB\)$/ })).toBeInTheDocument();
    });
  });
});
