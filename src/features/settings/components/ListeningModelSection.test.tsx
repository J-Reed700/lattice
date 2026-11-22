import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiProvider } from '@/contexts/ApiContext';
import type { TranscriptionStatusDto } from '@/lib/bindings';
import { createTestApi } from '@/tests/apiClient';
import type { DownloadStatus } from '@/types/downloads';

import { ListeningModelSection } from './ListeningModelSection';
import { startModelDownload } from '../hooks/startModelDownload';

const h = vi.hoisted(() => ({ downloads: [] as DownloadStatus[], settled: true }));

vi.mock('../hooks/startModelDownload', () => ({ startModelDownload: vi.fn(async () => ({ alreadyDownloaded: false })) }));
vi.mock('@/features/model/hooks/useDownloadState', () => ({
  useDownloadState: () => ({
    getDownloadsByModel: (modelId: string) => h.downloads.filter((download) => download.model_id === modelId),
    downloadsSettled: h.settled,
  }),
}));

const status = (modelReady: boolean): TranscriptionStatusDto => ({
  modelReady,
  modelName: modelReady ? 'Whisper Medium (distilled)' : null,
  modelId: modelReady ? 'whisper-medium-distilled' : null,
  voiceModel: { modelId: 'whisper-tiny-q8', modelName: 'Whisper Tiny', downloadBytes: 41_841_632, license: 'MIT' },
});

function setup(getTranscriptionStatus: () => Promise<unknown>) {
  const api = createTestApi({
    getTranscriptionStatus: getTranscriptionStatus as never,
    getDownloadedModels: vi.fn(async () => ({ ok: true as const, data: [] })) as never,
    deleteDownloadedModel: vi.fn(async () => ({ ok: true as const, data: undefined })) as never,
  });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <ApiProvider client={api}>
      <QueryClientProvider client={client}>
        <ListeningModelSection />
      </QueryClientProvider>
    </ApiProvider>,
  );
  return { api, queryClient: client, user: userEvent.setup() };
}

const answers = (modelReady: boolean) => vi.fn(async () => ({ ok: true as const, data: status(modelReady) }));

describe('ListeningModelSection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.downloads = [];
    h.settled = true;
  });

  it('offers Whisper for voice conversations, with its size and licence', async () => {
    const { user, queryClient } = setup(answers(false));
    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');
    const download = await screen.findByRole('button', { name: 'Download the listening model' });
    await waitFor(() => expect(download).toBeEnabled());
    expect(screen.getByText('Whisper Tiny')).toBeInTheDocument();
    expect(screen.getByText(/^Hears you when you press the mic in a chat\. .* once, then it works offline\. MIT licence\.$/)).toBeInTheDocument();

    await user.click(download);
    expect(startModelDownload).toHaveBeenCalledWith(
      expect.objectContaining({ metadata: expect.objectContaining({ id: 'whisper-tiny-q8', category: 'Transcription' }) }),
      expect.anything(),
    );
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['downloads'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['transcription', 'status'] });
  });

  it('names the listening model that is already here', async () => {
    setup(answers(true));
    expect(await screen.findByText('Whisper Medium (distilled)')).toBeInTheDocument();
    expect(await screen.findByText('Downloaded')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Download the listening model' })).not.toBeInTheDocument();
  });

  it('removes the record of the model in use, then offers the download again', async () => {
    let ready = true;
    const getStatus = vi.fn(async () => ({ ok: true as const, data: status(ready) }));
    const { api, user } = setup(getStatus);
    vi.mocked(api.getDownloadedModels).mockResolvedValue({
      ok: true,
      data: [
        { id: 'rec-tiny', model_id: 'whisper-tiny-q8' },
        { id: 'rec-medium', model_id: 'whisper-medium-distilled' },
      ],
    } as never);
    vi.mocked(api.deleteDownloadedModel).mockImplementation((async () => {
      ready = false;
      return { ok: true, data: undefined };
    }) as never);

    await user.click(await screen.findByRole('button', { name: 'Remove Whisper Medium (distilled)' }));
    await user.click(await screen.findByRole('button', { name: /^Remove\s*⌘/ }));

    expect(await screen.findByRole('button', { name: 'Download the listening model' })).toBeInTheDocument();
    expect(api.deleteDownloadedModel).toHaveBeenCalledWith('rec-medium', true);
  });

  it('shows Whisper arriving', async () => {
    h.downloads = [
      { model_id: 'whisper-tiny-q8', state: 'Downloading', total_bytes: 100, bytes_downloaded: 50, bytes_per_second: 1, percentage: 50 },
    ] as DownloadStatus[];
    setup(answers(false));
    expect(await screen.findByRole('status', { name: 'Downloading the listening model' })).toHaveTextContent('Downloading… 50%');
  });

  it('says why the last download failed, beside the Download button', async () => {
    h.downloads = [
      { model_id: 'whisper-tiny-q8', state: 'Failed', error_message: 'Disk full.', total_bytes: 100, bytes_downloaded: 10, bytes_per_second: 0 },
    ] as DownloadStatus[];
    setup(answers(false));
    expect(await screen.findByText("Couldn't download: Disk full.")).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Download the listening model' })).toBeInTheDocument();
  });

  it('holds the button until the download list has settled', async () => {
    h.settled = false;
    setup(answers(false));
    expect(await screen.findByRole('button', { name: 'Download the listening model' })).toBeDisabled();
  });

  it('offers Resume for a paused download', async () => {
    h.downloads = [
      { id: 'dl-1', model_id: 'whisper-tiny-q8', state: 'Paused', total_bytes: 100, bytes_downloaded: 50, bytes_per_second: 0, percentage: 50 },
    ] as DownloadStatus[];
    setup(answers(false));
    const region = await screen.findByRole('status', { name: 'Downloading the listening model' });
    expect(region).toHaveTextContent('Paused');
    expect(screen.getByRole('button', { name: 'Resume' })).toBeInTheDocument();
  });

  it('keeps reading the status while Whisper finishes, and offers the download again if it never lands', async () => {
    const recently = new Date(Date.now() - 58_000).toISOString();
    h.downloads = [
      { model_id: 'whisper-tiny-q8', state: 'Completed', total_bytes: 100, bytes_downloaded: 100, bytes_per_second: 0, completed_at: recently },
    ] as DownloadStatus[];
    const getStatus = answers(false);
    setup(getStatus);
    expect(await screen.findByRole('status', { name: 'Downloading the listening model' })).toHaveTextContent('Finishing…');
    await waitFor(() => expect(getStatus.mock.calls.length).toBeGreaterThan(1), { timeout: 3000 });
    expect(await screen.findByRole('button', { name: 'Download the listening model' }, { timeout: 5000 })).toBeInTheDocument();
  }, 10_000);

  it('says so when it can’t tell whether Whisper is here, and can try again', async () => {
    const getStatus = vi.fn(async (): Promise<unknown> => ({ ok: false, error: 'No backend' }));
    const { user } = setup(getStatus);
    await screen.findByRole('alert');

    getStatus.mockResolvedValue({ ok: true, data: status(true) });
    await user.click(screen.getByRole('button', { name: 'Try again' }));

    expect(await screen.findByText('Whisper Medium (distilled)')).toBeInTheDocument();
  });
});
