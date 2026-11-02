import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiProvider } from '@/contexts/ApiContext';
import { speechPlayer } from '@/features/speech/model/speechPlayer';
import type { ApiClient } from '@/lib/api';
import { conversationKeys } from '@/shared/conversations/conversationKeys';

import { useVoiceConversation } from './useVoiceConversation';
import { FRAME_SAMPLES } from '../model/micProcessor';

import type { MicHandlers } from '../model/micInput';

const h = vi.hoisted(() => ({
  store: {
    cancelGeneration: vi.fn(async () => undefined),
    optimisticMessages: new Map<string, { role: string; conversationId: string; content: string }>(),
    inFlightGenerations: new Map<string, string>(),
  },
  listeners: new Set<() => void>(),
  navigate: vi.fn(),
  mic: null as MicHandlers | null,
  speak: vi.fn(async (_sourceId: string, _pieces: AsyncIterable<string>) => undefined),
  stopMic: vi.fn(),
  openMic: vi.fn(),
  toast: { info: vi.fn(), error: vi.fn() },
}));

vi.mock('@/shared/conversations/conversationsStore', async () => {
  const { useSyncExternalStore } = await import('react');
  return {
    useConversationsStore: (select: (state: unknown) => unknown) =>
      useSyncExternalStore(
        (listener) => {
          h.listeners.add(listener);
          return () => h.listeners.delete(listener);
        },
        () => select(h.store),
      ),
  };
});
vi.mock('react-router', async (original) => ({
  ...(await original<typeof import('react-router')>()),
  useNavigate: () => h.navigate,
}));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));
vi.mock('@/features/speech/hooks/useSpeech', async (original) => ({
  ...(await original<typeof import('@/features/speech/hooks/useSpeech')>()),
  useSpeaker: () => h.speak,
}));
vi.mock('../model/micInput', async (original) => ({
  ...(await original<typeof import('../model/micInput')>()),
  openMic: h.openMic,
}));

const WHISPER = { modelId: 'whisper-tiny-q8', modelName: 'Whisper Tiny', downloadBytes: 41_841_632, license: 'MIT' };
const QUIET = new Float32Array(FRAME_SAMPLES);
const LOUD = new Float32Array(FRAME_SAMPLES).fill(0.05);

function hear(frame: Float32Array, count: number) {
  act(() => {
    for (let i = 0; i < count; i += 1) h.mic?.onFrame(frame);
  });
}

function publish(change: () => void) {
  act(() => {
    change();
    h.listeners.forEach((listener) => listener());
  });
}

function mockApi() {
  return {
    getTranscriptionStatus: vi.fn(),
    getSpeechStatus: vi.fn(),
    transcribeSpeech: vi.fn(),
  } as unknown as ApiClient;
}

function setup(api: ApiClient = mockApi()) {
  const queryClient = new QueryClient();
  const send = vi.fn(async (): Promise<'answered' | undefined> => 'answered');
  const fill = vi.fn();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>
      <ApiProvider client={api}>{children}</ApiProvider>
    </QueryClientProvider>
  );
  const view = renderHook(
    ({ conversationId }) => useVoiceConversation({ conversationId, send, fill }),
    { wrapper, initialProps: { conversationId: 'conversation-1' as string | null } },
  );
  return { ...view, api, queryClient, send, fill };
}

const readyApi = () => {
  const api = mockApi();
  vi.mocked(api.getTranscriptionStatus).mockResolvedValue({ ok: true, data: { modelReady: true, modelName: 'Whisper Tiny', voiceModel: WHISPER } });
  vi.mocked(api.getSpeechStatus).mockResolvedValue({ ok: true, data: { modelReady: true, modelName: 'Kokoro' } as never });
  vi.mocked(api.transcribeSpeech).mockResolvedValue({ ok: true, data: { text: 'Hello there.', language: 'en', durationMs: 1_000 } });
  return api;
};

async function start(view: ReturnType<typeof setup>) {
  act(() => view.result.current.toggle());
  await waitFor(() => expect(view.result.current.phase).toBe('listening'));
}

beforeEach(() => {
  vi.clearAllMocks();
  h.mic = null;
  h.store.optimisticMessages = new Map();
  h.store.inFlightGenerations = new Map();
  h.openMic.mockImplementation(async (handlers: MicHandlers) => {
    h.mic = handlers;
    return { stop: h.stopMic };
  });
  vi.spyOn(speechPlayer, 'whenIdle').mockResolvedValue(undefined);
});

describe('useVoiceConversation', () => {
  it('starts off, and listens once pressed with everything ready', async () => {
    const view = setup(readyApi());
    expect(view.result.current.phase).toBe('off');
    await start(view);
    expect(h.openMic).toHaveBeenCalled();
  });

  it('points at settings rather than failing when Whisper is not downloaded', async () => {
    const api = readyApi();
    vi.mocked(api.getTranscriptionStatus).mockResolvedValue({ ok: true, data: { modelReady: false, modelName: null, voiceModel: WHISPER } });
    const view = setup(api);

    act(() => view.result.current.toggle());

    await waitFor(() => expect(h.toast.info).toHaveBeenCalled());
    const [title, options] = h.toast.info.mock.calls[0];
    expect(title).toBe('Download Whisper first');
    options.action.onClick();
    expect(h.navigate).toHaveBeenCalledWith('/settings?tab=read-aloud');
    expect(h.openMic).not.toHaveBeenCalled();
    await waitFor(() => expect(view.result.current.phase).toBe('off'));
  });

  it('points at settings when the voices are not downloaded, as read aloud does', async () => {
    const api = readyApi();
    vi.mocked(api.getSpeechStatus).mockResolvedValue({ ok: true, data: { modelReady: false, modelName: 'Kokoro' } as never });
    const view = setup(api);

    act(() => view.result.current.toggle());

    await waitFor(() => expect(h.toast.info).toHaveBeenCalledWith(
      'Download the read-aloud voices first',
      expect.objectContaining({ message: 'Kokoro runs on this computer.' }),
    ));
    expect(h.openMic).not.toHaveBeenCalled();
  });

  it('says what went wrong when the checks themselves fail', async () => {
    const api = readyApi();
    vi.mocked(api.getTranscriptionStatus).mockResolvedValue({ ok: false, error: 'No backend' } as never);
    const view = setup(api);

    act(() => view.result.current.toggle());

    await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith(
      "Couldn't start the voice conversation",
      { message: 'No backend' },
    ));
  });

  it('ends on the second press and lets the mic go', async () => {
    const view = setup(readyApi());
    await start(view);

    act(() => view.result.current.toggle());

    expect(view.result.current.phase).toBe('off');
    expect(h.stopMic).toHaveBeenCalled();
  });

  it('ends when the chat changes, so nothing speaks into the wrong conversation', async () => {
    const view = setup(readyApi());
    await start(view);

    view.rerender({ conversationId: 'conversation-2' });

    expect(h.stopMic).toHaveBeenCalled();
    await waitFor(() => expect(view.result.current.phase).toBe('off'));
  });

  it('ends when the chat closes', async () => {
    const view = setup(readyApi());
    await start(view);
    view.unmount();
    expect(h.stopMic).toHaveBeenCalled();
  });

  it('sends what Whisper heard, as 16-bit PCM', async () => {
    const view = setup(readyApi());
    await start(view);

    hear(QUIET, 5);
    hear(LOUD, 20);
    hear(QUIET, 40);

    await waitFor(() => expect(view.send).toHaveBeenCalledWith('Hello there.'));
    const [pcm] = vi.mocked(view.api.transcribeSpeech).mock.calls[0];
    expect(atob(pcm).length % 2).toBe(0);
    expect(atob(pcm).length).toBeGreaterThan(16_000);
  });

  it('speaks the reply as it is written, and then the end of what was saved', async () => {
    const view = setup(readyApi());
    const turn = { finish: (_: 'answered') => {} };
    view.send.mockImplementation(() => new Promise((resolve) => { turn.finish = resolve; }));
    await start(view);
    hear(QUIET, 5);
    hear(LOUD, 20);
    hear(QUIET, 40);
    await waitFor(() => expect(view.send).toHaveBeenCalled());

    publish(() => h.store.optimisticMessages.set('temp', { role: 'assistant', conversationId: 'conversation-1', content: 'One here. Two' }));
    await waitFor(() => expect(view.result.current.phase).toBe('speaking'));
    const [sourceId, pieces] = h.speak.mock.calls[0];
    expect(sourceId).toBe('voice-reply');

    view.queryClient.setQueryData(conversationKeys.messages('conversation-1'), [
      { id: 'm1', role: 'user', content: 'Hello there.' },
      { id: 'm2', role: 'assistant', content: 'One here. Two here.' },
    ]);
    turn.finish('answered');

    const said: string[] = [];
    for await (const piece of pieces) said.push(piece);
    expect(said).toEqual(['One here.', 'Two here.']);
    await waitFor(() => expect(view.result.current.phase).toBe('listening'));
  });

  it('ignores a reply being written in another chat', async () => {
    const view = setup(readyApi());
    const pending = new Promise<undefined>(() => {});
    view.send.mockReturnValue(pending);
    await start(view);
    hear(QUIET, 5);
    hear(LOUD, 20);
    hear(QUIET, 40);
    await waitFor(() => expect(view.send).toHaveBeenCalled());

    publish(() => h.store.optimisticMessages.set('temp', { role: 'assistant', conversationId: 'elsewhere', content: 'One here. Two' }));

    expect(h.speak).not.toHaveBeenCalled();
  });

  it('stops the turn and waits for the chat before sending what interrupted it', async () => {
    const view = setup(readyApi());
    view.send.mockImplementationOnce(() => new Promise(() => {}));
    await start(view);
    hear(QUIET, 5);
    hear(LOUD, 20);
    hear(QUIET, 40);
    await waitFor(() => expect(view.send).toHaveBeenCalledTimes(1));
    publish(() => {
      h.store.inFlightGenerations.set('conversation-1', 'request-1');
      h.store.optimisticMessages.set('temp', { role: 'assistant', conversationId: 'conversation-1', content: 'One here. Two' });
    });
    await waitFor(() => expect(view.result.current.phase).toBe('speaking'));

    hear(LOUD, 20);
    expect(h.store.cancelGeneration).toHaveBeenCalledWith('conversation-1');
    hear(QUIET, 40);
    await waitFor(() => expect(view.result.current.phase).toBe('thinking'));
    expect(view.send).toHaveBeenCalledTimes(1);

    publish(() => {
      h.store.inFlightGenerations.delete('conversation-1');
      h.store.optimisticMessages.clear();
    });

    await waitFor(() => expect(view.send).toHaveBeenCalledTimes(2));
  });

  it('keeps the words in the composer when the chat can’t take them', async () => {
    const view = setup(readyApi());
    view.send.mockResolvedValue(undefined);
    await start(view);
    hear(QUIET, 5);
    hear(LOUD, 20);
    hear(QUIET, 40);

    await waitFor(() => expect(h.toast.error).toHaveBeenCalled());
    expect(view.fill).toHaveBeenCalledWith('Hello there.');
    expect(h.toast.error.mock.calls[0][1].message).toMatch(/message box/);
    await waitFor(() => expect(view.result.current.phase).toBe('off'));
  });

  it('says so when Whisper fails', async () => {
    const api = readyApi();
    vi.mocked(api.transcribeSpeech).mockResolvedValue({ ok: false, error: 'Whisper broke' } as never);
    const view = setup(api);
    await start(view);
    hear(QUIET, 5);
    hear(LOUD, 20);
    hear(QUIET, 40);

    await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith(
      "Couldn't work out what you said",
      { message: 'Whisper broke' },
    ));
  });

  it('stops speech that belongs to it, and only that', async () => {
    const stop = vi.spyOn(speechPlayer, 'stop');
    const view = setup(readyApi());
    const pending = new Promise<undefined>(() => {});
    view.send.mockReturnValue(pending);
    await start(view);
    hear(QUIET, 5);
    hear(LOUD, 20);
    hear(QUIET, 40);
    await waitFor(() => expect(view.send).toHaveBeenCalled());

    act(() => view.result.current.toggle());

    expect(stop).toHaveBeenCalledWith('voice-reply');
  });
});
