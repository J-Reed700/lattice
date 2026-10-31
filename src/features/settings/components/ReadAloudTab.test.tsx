import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiProvider } from '@/contexts/ApiContext';
import { speechPlayer } from '@/features/speech/model/speechPlayer';
import { useSpeechPrefsStore } from '@/features/speech/stores/speechPrefsStore';
import type { SpeechStatusDto } from '@/lib/bindings';
import { createTestApi } from '@/tests/apiClient';
import type { DownloadStatus } from '@/types/downloads';

import { ReadAloudTab } from './ReadAloudTab';
import { startModelDownload } from '../hooks/startModelDownload';

const h = vi.hoisted(() => ({ downloads: [] as DownloadStatus[] }));

vi.mock('../hooks/startModelDownload', () => ({ startModelDownload: vi.fn(async () => ({ alreadyDownloaded: false })) }));
vi.mock('@/features/model/hooks/useDownloadState', () => ({
  useDownloadState: () => ({ getDownloadsByModel: () => h.downloads }),
}));
vi.mock('@/features/speech/model/speechPlayer', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/features/speech/model/speechPlayer')>();
  const output = {
    currentTime: 0,
    state: 'running',
    destination: {},
    resume: async () => {},
    suspend: async () => {},
    createBuffer: (_c: number, length: number, rate: number) => ({ duration: length / rate, copyToChannel: () => {} }),
    createBufferSource: () => ({ connect: () => {}, start: () => {}, stop: () => {}, onended: null, buffer: null }),
  };
  return { ...actual, speechPlayer: new actual.SpeechPlayer(() => output as never) };
});

const status = (modelReady: boolean): SpeechStatusDto => ({
  modelReady,
  modelId: 'kokoro-82m-british',
  modelName: 'Kokoro 82M (British voices)',
  downloadBytes: 338_835_287,
  license: 'Apache-2.0',
  voices: [
    { id: 'bf_emma', name: 'Emma' },
    { id: 'bf_isabella', name: 'Isabella' },
    { id: 'bf_alice', name: 'Alice' },
    { id: 'bf_lily', name: 'Lily' },
  ],
  defaultVoice: 'bf_emma',
});

function setup(modelReady: boolean) {
  const api = createTestApi({
    getSpeechStatus: vi.fn(async () => ({ ok: true as const, data: status(modelReady) })),
    synthesizeSpeech: vi.fn(async () => ({ ok: true as const, data: { sampleRate: 10, pcm: btoa('\0\0'), durationMs: 100 } })),
  });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <ApiProvider client={api}>
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <ReadAloudTab />
        </MemoryRouter>
      </QueryClientProvider>
    </ApiProvider>,
  );
  return { api, user: userEvent.setup() };
}

describe('ReadAloudTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.downloads = [];
    speechPlayer.stop();
    useSpeechPrefsStore.setState({ voice: 'bf_emma', speed: 1 });
  });

  it('offers the voice model download with its size and licence', async () => {
    const { user } = setup(false);
    const download = await screen.findByRole('button', { name: 'Download' });
    expect(screen.getByText('323.14 MB once, then it works offline. Apache-2.0 licence.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Preview Emma' })).toBeDisabled();

    await user.click(download);
    expect(startModelDownload).toHaveBeenCalledWith(
      expect.objectContaining({ metadata: expect.objectContaining({ id: 'kokoro-82m-british', category: 'Speech' }) }),
      expect.anything(),
    );
  });

  it('shows download progress while the files arrive', async () => {
    h.downloads = [
      { model_id: 'kokoro-82m-british', state: 'Downloading', total_bytes: 300, bytes_downloaded: 150, bytes_per_second: 1 },
      { model_id: 'kokoro-82m-british', state: 'Pending', total_bytes: 100, bytes_downloaded: 0, bytes_per_second: 0 },
    ] as DownloadStatus[];
    setup(false);
    expect(await screen.findByText('Downloading… 38%')).toBeInTheDocument();
  });

  it('picks a British voice, previews it, and sets the pace', async () => {
    const { api, user } = setup(true);
    expect(await screen.findByText('Downloaded')).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: /Emma/ })).toBeChecked();

    await user.click(screen.getByRole('radio', { name: /Isabella/ }));
    expect(useSpeechPrefsStore.getState().voice).toBe('bf_isabella');

    await user.click(screen.getByRole('button', { name: 'Preview Lily' }));
    await waitFor(() =>
      expect(api.synthesizeSpeech).toHaveBeenCalledWith(
        "Hello, I'm Lily. I can read your answers and lessons aloud.",
        'bf_lily',
        1,
      ),
    );

    await user.selectOptions(screen.getByLabelText('Speed'), '1.25');
    expect(useSpeechPrefsStore.getState().speed).toBe(1.25);
  });
});
