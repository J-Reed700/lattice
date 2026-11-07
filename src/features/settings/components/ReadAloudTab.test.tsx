import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
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
  useDownloadState: () => ({ getDownloadsByModel: (id: string) => h.downloads.filter((download) => download.model_id === id) }),
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

const status = (modelReady: boolean, packReady = false): SpeechStatusDto => {
  const voice = (id: string, name: string, accent: 'american' | 'british', gender: 'female' | 'male', grade: string, inPack: boolean) => ({
    id, name, accent, gender, grade, downloaded: modelReady && (!inPack || packReady),
  });
  return {
    modelReady,
    modelId: 'kokoro-82m-british',
    modelName: 'Kokoro 82M',
    downloadBytes: 338_835_287,
    license: 'Apache-2.0',
    voicePack: { ready: packReady, modelId: 'kokoro-82m-more-voices', modelName: 'Kokoro 82M: more voices', downloadBytes: 21_653_257 },
    voices: [
      voice('bf_emma', 'Emma', 'british', 'female', 'B-', false),
      voice('bf_isabella', 'Isabella', 'british', 'female', 'C', false),
      voice('bm_george', 'George', 'british', 'male', 'C', true),
      voice('bf_lily', 'Lily', 'british', 'female', 'D', false),
      voice('af_heart', 'Heart', 'american', 'female', 'A', true),
      voice('am_michael', 'Michael', 'american', 'male', 'C+', true),
    ],
    defaultVoice: 'bf_emma',
  };
};

function setup(modelReady: boolean, packReady = false) {
  const api = createTestApi({
    getSpeechStatus: vi.fn(async () => ({ ok: true as const, data: status(modelReady, packReady) })),
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
    const download = await screen.findByRole('button', { name: 'Download Kokoro 82M' });
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

  it('tells an install with the first download what the rest costs, and fetches only that', async () => {
    const { user } = setup(true);
    const download = await screen.findByRole('button', { name: 'Download Kokoro 82M: more voices' });
    expect(screen.getByText('Downloaded')).toBeInTheDocument();
    expect(screen.getByText(/20\.65 MB more\. Kokoro 82M is not fetched again\./)).toBeInTheDocument();

    await user.click(download);
    expect(startModelDownload).toHaveBeenCalledTimes(1);
    expect(startModelDownload).toHaveBeenCalledWith(
      expect.objectContaining({ metadata: expect.objectContaining({ id: 'kokoro-82m-more-voices', category: 'Speech' }) }),
      expect.anything(),
    );
  });

  it('shows the pack’s progress on its own row', async () => {
    h.downloads = [
      { model_id: 'kokoro-82m-more-voices', state: 'Downloading', total_bytes: 300, bytes_downloaded: 100, bytes_per_second: 1 },
      { model_id: 'kokoro-82m-more-voices', state: 'Pending', total_bytes: 100, bytes_downloaded: 0, bytes_per_second: 0 },
    ] as DownloadStatus[];
    setup(true);
    expect(await screen.findByText('Downloading… 25%')).toBeInTheDocument();
    expect(screen.getByText('Downloaded')).toBeInTheDocument();
  });

  it('groups the voices by accent, each with its gender and grade', async () => {
    setup(true, true);
    const british = await screen.findByRole('group', { name: 'British English' });
    const american = screen.getByRole('group', { name: 'American English' });
    expect(within(british).getAllByRole('radio').map((radio) => (radio as HTMLInputElement).value)).toEqual(['bf_emma', 'bf_isabella', 'bm_george', 'bf_lily']);
    expect(within(american).getAllByRole('radio').map((radio) => (radio as HTMLInputElement).value)).toEqual(['af_heart', 'am_michael']);
    expect(within(american).getByRole('radio', { name: /Heart/ })).toHaveAccessibleName('Heart Female · grade A');
    expect(within(american).getByRole('radio', { name: /Michael/ })).toHaveAccessibleName('Michael Male · grade C+');
    expect(within(british).getByRole('radio', { name: /Emma/ })).toHaveAccessibleName('Emma Female · grade B- · default');
  });

  it('keeps a voice the install does not have from being picked or previewed', async () => {
    const { api, user } = setup(true);
    const heart = await screen.findByRole('radio', { name: /Heart/ });
    expect(heart).toBeDisabled();
    expect(heart).toHaveAccessibleName('Heart Female · grade A · not downloaded');
    expect(screen.getByRole('radio', { name: /George/ })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Preview Heart' })).toBeDisabled();
    expect(screen.getByRole('radio', { name: /Emma/ })).toBeEnabled();

    await user.click(heart);
    expect(useSpeechPrefsStore.getState().voice).toBe('bf_emma');
    expect(api.synthesizeSpeech).not.toHaveBeenCalled();
  });

  it('picks an American voice once the pack is here', async () => {
    const { api, user } = setup(true, true);
    await user.click(await screen.findByRole('radio', { name: /Heart/ }));
    expect(useSpeechPrefsStore.getState().voice).toBe('af_heart');

    await user.click(screen.getByRole('button', { name: 'Preview Michael' }));
    await waitFor(() =>
      expect(api.synthesizeSpeech).toHaveBeenCalledWith(
        "Hello, I'm Michael. I can read your answers and lessons aloud.",
        'am_michael',
        1,
      ),
    );
  });

  it('picks a British voice, previews it, and sets the pace', async () => {
    const { api, user } = setup(true);
    expect(await screen.findByRole('button', { name: 'Download Kokoro 82M: more voices' })).toBeInTheDocument();
    expect(screen.getByText('Downloaded')).toBeInTheDocument();
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
