import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiProvider } from '@/contexts/ApiContext';
import type { SpeechStatusDto } from '@/lib/bindings';
import { useToastStore } from '@/stores/toastStore';
import { createTestApi } from '@/tests/apiClient';

import { ListenButton } from './ListenButton';
import { SPEECH_STATUS_QUERY_KEY } from '../hooks/useSpeech';
import { speechPlayer } from '../model/speechPlayer';

const output = vi.hoisted(() => ({
  currentTime: 0,
  state: 'running',
  destination: {},
  resume: async () => {},
  suspend: async () => {},
  createBuffer: (_channels: number, length: number, sampleRate: number) => ({ duration: length / sampleRate, copyToChannel: () => {} }),
  createBufferSource: () => ({ connect: () => {}, start: () => {}, stop: () => {}, onended: null, buffer: null }),
}));

vi.mock('../model/speechPlayer', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../model/speechPlayer')>();
  return { ...actual, speechPlayer: new actual.SpeechPlayer(() => output as never) };
});

const status = (modelReady: boolean): SpeechStatusDto => ({
  modelReady,
  modelId: 'kokoro-82m-british',
  modelName: 'Kokoro 82M (British voices)',
  downloadBytes: 339_000_000,
  license: 'Apache-2.0',
  voices: [{ id: 'bf_emma', name: 'Emma' }],
  defaultVoice: 'bf_emma',
});

const clip = { sampleRate: 10, pcm: btoa('\0\0\0\0'), durationMs: 200 };

function setup(modelReady = true) {
  const api = createTestApi({
    getSpeechStatus: vi.fn(async () => ({ ok: true as const, data: status(modelReady) })),
    synthesizeSpeech: vi.fn(async () => ({ ok: true as const, data: clip })),
  });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <ApiProvider client={api}>
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <ListenButton sourceId="message:1" markdown={'First sentence [1]. Second `code` sentence.\n\n```\nskip()\n```'} subject="this answer" />
          <ListenButton sourceId="message:2" markdown="Another answer." subject="that answer" />
        </MemoryRouter>
      </QueryClientProvider>
    </ApiProvider>,
  );
  return { api, client, user: userEvent.setup() };
}

describe('ListenButton', () => {
  beforeEach(() => {
    speechPlayer.stop();
    useToastStore.setState({ toasts: [] });
  });

  it('reads the answer’s prose aloud, then pauses, resumes and stops', async () => {
    const { api, user } = setup();
    await waitFor(() => expect(api.getSpeechStatus).toHaveBeenCalled());

    await user.click(screen.getByRole('button', { name: 'Listen to this answer' }));
    await screen.findByRole('button', { name: 'Pause reading this answer' });
    expect(api.synthesizeSpeech).toHaveBeenNthCalledWith(1, 'First sentence.', 'bf_emma', 1);
    expect(api.synthesizeSpeech).toHaveBeenNthCalledWith(2, 'Second code sentence.', 'bf_emma', 1);

    await user.click(screen.getByRole('button', { name: 'Pause reading this answer' }));
    await user.click(screen.getByRole('button', { name: 'Resume reading this answer' }));
    await user.click(screen.getByRole('button', { name: 'Stop reading this answer' }));
    expect(screen.getByRole('button', { name: 'Listen to this answer' })).toBeInTheDocument();
  });

  it('speaks one thing at a time', async () => {
    const { user } = setup();
    await user.click(screen.getByRole('button', { name: 'Listen to this answer' }));
    await screen.findByRole('button', { name: 'Stop reading this answer' });

    await user.click(screen.getByRole('button', { name: 'Listen to that answer' }));
    await screen.findByRole('button', { name: 'Stop reading that answer' });
    expect(screen.getByRole('button', { name: 'Listen to this answer' })).toBeInTheDocument();
  });

  it('points to Settings instead of speaking when the voices are not downloaded', async () => {
    const { api, client, user } = setup(false);
    await waitFor(() => expect(client.getQueryData(SPEECH_STATUS_QUERY_KEY)).toBeDefined());

    await user.click(screen.getByRole('button', { name: 'Listen to this answer' }));
    await waitFor(() => expect(useToastStore.getState().toasts).toHaveLength(1));
    const [toast] = useToastStore.getState().toasts;
    expect(toast.title).toBe('Download the read-aloud voices first');
    expect(toast.action?.label).toBe('Open settings');
    expect(api.synthesizeSpeech).not.toHaveBeenCalled();
  });
});
