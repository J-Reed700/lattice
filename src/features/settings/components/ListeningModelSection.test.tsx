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

const h = vi.hoisted(() => ({ downloads: [] as DownloadStatus[] }));

vi.mock('../hooks/startModelDownload', () => ({ startModelDownload: vi.fn(async () => ({ alreadyDownloaded: false })) }));
vi.mock('@/features/model/hooks/useDownloadState', () => ({
  useDownloadState: () => ({
    getDownloadsByModel: (modelId: string) => h.downloads.filter((download) => download.model_id === modelId),
  }),
}));

const status = (modelReady: boolean): TranscriptionStatusDto => ({
  modelReady,
  modelName: modelReady ? 'Whisper Medium (distilled)' : null,
  voiceModel: { modelId: 'whisper-tiny-q8', modelName: 'Whisper Tiny', downloadBytes: 41_841_632, license: 'MIT' },
});

function setup(getTranscriptionStatus: () => Promise<unknown>) {
  const api = createTestApi({ getTranscriptionStatus: getTranscriptionStatus as never });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <ApiProvider client={api}>
      <QueryClientProvider client={client}>
        <ListeningModelSection />
      </QueryClientProvider>
    </ApiProvider>,
  );
  return { api, user: userEvent.setup() };
}

const answers = (modelReady: boolean) => vi.fn(async () => ({ ok: true as const, data: status(modelReady) }));

describe('ListeningModelSection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.downloads = [];
  });

  it('offers Whisper for voice conversations, with its size and licence', async () => {
    const { user } = setup(answers(false));
    const download = await screen.findByRole('button', { name: 'Download the listening model' });
    expect(screen.getByText('Whisper Tiny')).toBeInTheDocument();
    expect(screen.getByText(/^Hears you when you press the mic in a chat\. .* once, then it works offline\. MIT licence\.$/)).toBeInTheDocument();

    await user.click(download);
    expect(startModelDownload).toHaveBeenCalledWith(
      expect.objectContaining({ metadata: expect.objectContaining({ id: 'whisper-tiny-q8', category: 'Transcription' }) }),
      expect.anything(),
    );
  });

  it('names the listening model that is already here', async () => {
    setup(answers(true));
    expect(await screen.findByText('Whisper Medium (distilled)')).toBeInTheDocument();
    expect(await screen.findByText('Downloaded')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Download the listening model' })).not.toBeInTheDocument();
  });

  it('shows Whisper arriving', async () => {
    h.downloads = [
      { model_id: 'whisper-tiny-q8', state: 'Downloading', total_bytes: 100, bytes_downloaded: 50, bytes_per_second: 1, percentage: 50 },
    ] as DownloadStatus[];
    setup(answers(false));
    expect(await screen.findByRole('status', { name: 'Downloading the listening model' })).toHaveTextContent('Downloading… 50%');
  });

  it('reads the status again once Whisper has arrived', async () => {
    h.downloads = [
      { model_id: 'whisper-tiny-q8', state: 'Completed', total_bytes: 100, bytes_downloaded: 100, bytes_per_second: 0 },
    ] as DownloadStatus[];
    const getStatus = answers(false);
    setup(getStatus);
    await waitFor(() => expect(getStatus.mock.calls.length).toBeGreaterThan(1));
  });

  it('says so when it can’t tell whether Whisper is here, and can try again', async () => {
    const getStatus = vi.fn(async (): Promise<unknown> => ({ ok: false, error: 'No backend' }));
    const { user } = setup(getStatus);
    await screen.findByRole('alert');

    getStatus.mockResolvedValue({ ok: true, data: status(true) });
    await user.click(screen.getByRole('button', { name: 'Try again' }));

    expect(await screen.findByText('Whisper Medium (distilled)')).toBeInTheDocument();
  });
});
