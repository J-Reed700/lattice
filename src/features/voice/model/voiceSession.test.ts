import { describe, expect, it, vi } from 'vitest';

import type { GenerationOutcome } from '@/shared/conversations/conversationsStore.types';
import { deferred } from '@/tests/deferred';

import { MicError, type MicHandlers } from './micInput';
import { FRAME_SAMPLES } from './micProcessor';
import { VoiceSession, type VoicePhase, type VoiceSessionDeps } from './voiceSession';

const QUIET = new Float32Array(FRAME_SAMPLES);
const LOUD = new Float32Array(FRAME_SAMPLES).fill(0.05);

function setup(overrides: Partial<VoiceSessionDeps> = {}) {
  const phases: VoicePhase[] = [];
  const failures: Array<[string, string]> = [];
  let handlers: MicHandlers | null = null;
  const mic = { stop: vi.fn() };
  const turn = { outcome: deferred<GenerationOutcome>() };
  const heard: string[][] = [];
  const speech = { done: deferred<void>(), pieces: null as AsyncIterable<string> | null };
  const deps: VoiceSessionDeps = {
    ready: async () => true,
    openMic: async (given) => {
      handlers = given;
      return mic;
    },
    transcribe: vi.fn(async () => 'What is a monad?'),
    send: vi.fn(() => turn.outcome.promise),
    finalReply: () => '',
    interruptTurn: vi.fn(),
    whenFree: async () => undefined,
    speak: vi.fn(async (pieces: AsyncIterable<string>) => {
      speech.pieces = pieces;
      const said: string[] = [];
      heard.push(said);
      for await (const piece of pieces) said.push(piece);
      await speech.done.promise;
    }),
    stopSpeaking: vi.fn(() => speech.done.resolve()),
    isSpeaking: () => false,
    onPhase: (phase) => phases.push(phase),
    onFailure: (title, message) => failures.push([title, message]),
    ...overrides,
  };
  const session = new VoiceSession(deps);
  const frames = (frame: Float32Array, count: number) => {
    for (let i = 0; i < count; i += 1) handlers?.onFrame(frame);
  };
  /** Someone says something and stops, and it reaches the thinking stage. */
  const speak = async () => {
    frames(QUIET, 5);
    frames(LOUD, 20);
    frames(QUIET, 40);
    await vi.waitFor(() => expect(deps.send).toHaveBeenCalled());
  };
  return { session, deps, phases, failures, mic, turn, heard, speech, frames, speak, lose: () => handlers?.onLost() };
}

const last = (phases: VoicePhase[]) => phases[phases.length - 1];

describe('VoiceSession', () => {
  it('goes from the press to listening with the mic open', async () => {
    const { session, phases } = setup();
    await session.start();
    expect(phases).toEqual(['starting', 'listening']);
  });

  it('sends what was said, once it has been said', async () => {
    const { session, deps, phases, frames, speak } = setup();
    await session.start();
    frames(QUIET, 5);
    expect(deps.transcribe).not.toHaveBeenCalled();

    await speak();

    expect(phases).toEqual(['starting', 'listening', 'hearing', 'transcribing', 'thinking']);
    expect(vi.mocked(deps.transcribe).mock.calls[0][0].length).toBeGreaterThan(16_000);
    expect(deps.send).toHaveBeenCalledWith('What is a monad?');
  });

  it('starts speaking before the reply is finished, and listens again after it', async () => {
    const { session, phases, turn, heard, speech, speak } = setup({ finalReply: () => 'One here. Two here. Three here.' });
    await session.start();
    await speak();

    session.replyGrew('One here. Two');
    await vi.waitFor(() => expect(heard[0]).toEqual(['One here.']));
    expect(last(phases)).toBe('speaking');

    session.replyGrew('One here. Two here. Three');
    await vi.waitFor(() => expect(heard[0]).toEqual(['One here.', 'Two here.']));

    turn.outcome.resolve('answered');
    await vi.waitFor(() => expect(heard[0]).toEqual(['One here.', 'Two here.', 'Three here.']));
    expect(last(phases)).toBe('speaking');

    speech.done.resolve();
    await vi.waitFor(() => expect(last(phases)).toBe('listening'));
  });

  it('says a reply that only arrives at the end', async () => {
    const { session, phases, turn, heard, speech, speak } = setup({ finalReply: () => 'All at once.' });
    await session.start();
    await speak();

    turn.outcome.resolve('answered');
    await vi.waitFor(() => expect(heard[0]).toEqual(['All at once.']));
    speech.done.resolve();
    await vi.waitFor(() => expect(last(phases)).toBe('listening'));
  });

  it('listens again after a reply with nothing to say aloud', async () => {
    const { session, deps, phases, turn, speak } = setup({ finalReply: () => '```\ncode only\n```' });
    await session.start();
    await speak();

    turn.outcome.resolve('answered');
    await vi.waitFor(() => expect(last(phases)).toBe('listening'));
    expect(deps.speak).not.toHaveBeenCalled();
  });

  it('goes straight back to listening when nothing was really said', async () => {
    const { session, deps, phases, frames } = setup({ transcribe: async () => ' [BLANK_AUDIO] ' });
    await session.start();
    frames(QUIET, 5);
    frames(LOUD, 20);
    frames(QUIET, 40);

    await vi.waitFor(() => expect(last(phases)).toBe('listening'));
    expect(deps.send).not.toHaveBeenCalled();
  });

  it('sends nothing for noises Whisper writes in brackets', async () => {
    const { session, deps, phases, frames } = setup({ transcribe: async () => '(wind blowing) ♪' });
    await session.start();
    frames(QUIET, 5);
    frames(LOUD, 20);
    frames(QUIET, 40);

    await vi.waitFor(() => expect(last(phases)).toBe('listening'));
    expect(deps.send).not.toHaveBeenCalled();
  });

  it('keeps words that come with a bracket', async () => {
    const { session, deps, speak } = setup({ transcribe: async () => 'Compare it (roughly) to a list.' });
    await session.start();
    await speak();
    expect(deps.send).toHaveBeenCalledWith('Compare it (roughly) to a list.');
  });

  it('stops the reply and hears the speaker who talks over it', async () => {
    const { session, deps, phases, turn, heard, frames, speak } = setup();
    await session.start();
    await speak();
    session.replyGrew('One here. Two');
    await vi.waitFor(() => expect(heard[0]).toEqual(['One here.']));

    frames(LOUD, 20);

    expect(deps.stopSpeaking).toHaveBeenCalled();
    expect(deps.interruptTurn).toHaveBeenCalled();
    expect(last(phases)).toBe('hearing');

    // The stopped turn settling does not disturb the one now being heard.
    turn.outcome.resolve('cancelled');
    await Promise.resolve();
    expect(last(phases)).toBe('hearing');
  });

  it('sends what interrupted the reply once the chat is free', async () => {
    const free = deferred<void>();
    const { session, deps, heard, frames, turn, speak } = setup({ whenFree: vi.fn(() => free.promise) });
    free.resolve();
    await session.start();
    await speak();
    session.replyGrew('One here. Two');
    await vi.waitFor(() => expect(heard[0]).toEqual(['One here.']));

    const wait = deferred<void>();
    vi.mocked(deps.whenFree).mockReturnValue(wait.promise);
    vi.mocked(deps.transcribe).mockResolvedValueOnce('Hold on.');
    frames(LOUD, 20);
    frames(QUIET, 40);
    turn.outcome.resolve('cancelled');
    await vi.waitFor(() => expect(deps.whenFree).toHaveBeenCalledTimes(2));
    expect(deps.send).toHaveBeenCalledTimes(1);

    wait.resolve();
    await vi.waitFor(() => expect(deps.send).toHaveBeenCalledWith('Hold on.'));
  });

  it('does not say what is left of a stopped reply as the next one', async () => {
    const free = deferred<void>();
    const { session, deps, heard, frames, turn, speak } = setup();
    await session.start();
    await speak();
    session.replyGrew('One here. Two');
    await vi.waitFor(() => expect(heard[0]).toEqual(['One here.']));

    deps.whenFree = () => free.promise;
    frames(LOUD, 20);
    frames(QUIET, 40);
    await vi.waitFor(() => expect(deps.transcribe).toHaveBeenCalledTimes(2));
    await Promise.resolve();

    // The stopped turn's last words arrive while the new one waits for the chat.
    session.replyGrew('One here. Two here. Three here. Four');
    await Promise.resolve();
    expect(deps.speak).toHaveBeenCalledTimes(1);

    turn.outcome.resolve('cancelled');
    free.resolve();
  });

  it('needs more than a whisper of echo to count as an interruption', async () => {
    const { session, deps, heard, frames, speak } = setup();
    await session.start();
    await speak();
    session.replyGrew('One here. Two');
    await vi.waitFor(() => expect(heard[0]).toEqual(['One here.']));

    frames(new Float32Array(FRAME_SAMPLES).fill(0.002), 40);

    expect(deps.stopSpeaking).not.toHaveBeenCalled();
  });

  it('treats the reader’s own Listen playing as the app speaking', async () => {
    const { session, phases, frames } = setup({ isSpeaking: () => true });
    await session.start();
    frames(new Float32Array(FRAME_SAMPLES).fill(0.005), 40);
    expect(last(phases)).toBe('listening');
  });

  it('listens again when the turn is stopped from the chat', async () => {
    const { session, deps, phases, turn, heard, speak } = setup();
    await session.start();
    await speak();
    session.replyGrew('One here. Two');
    await vi.waitFor(() => expect(heard[0]).toEqual(['One here.']));

    turn.outcome.resolve('cancelled');

    await vi.waitFor(() => expect(last(phases)).toBe('listening'));
    expect(deps.stopSpeaking).toHaveBeenCalled();
  });

  it('ends with a message when the turn fails', async () => {
    const { session, failures, mic, phases, turn, speak } = setup();
    await session.start();
    await speak();

    turn.outcome.resolve('failed');

    await vi.waitFor(() => expect(failures).toHaveLength(1));
    expect(last(phases)).toBe('off');
    expect(mic.stop).toHaveBeenCalled();
  });

  it('ends with a message when the turn could not be sent', async () => {
    const { session, failures, phases, frames } = setup({ send: async () => { throw new Error('The chat is busy.'); } });
    await session.start();
    frames(QUIET, 5);
    frames(LOUD, 20);
    frames(QUIET, 40);

    await vi.waitFor(() => expect(failures).toEqual([["Couldn't send what you said", 'The chat is busy.']]));
    expect(last(phases)).toBe('off');
  });

  it('ends with a message when Whisper fails', async () => {
    const { session, failures, phases, frames } = setup({ transcribe: async () => { throw new Error('Whisper broke.'); } });
    await session.start();
    frames(QUIET, 5);
    frames(LOUD, 20);
    frames(QUIET, 40);

    await vi.waitFor(() => expect(failures).toEqual([["Couldn't work out what you said", 'Whisper broke.']]));
    expect(last(phases)).toBe('off');
  });

  it('shows why the mic failed rather than sitting there', async () => {
    const { session, failures, phases } = setup({ openMic: async () => { throw new MicError('denied'); } });
    await session.start();
    expect(failures[0][0]).toBe("Couldn't use the microphone");
    expect(failures[0][1]).toMatch(/Privacy & security/);
    expect(last(phases)).toBe('off');
  });

  it('ends with a message when the mic is pulled out', async () => {
    const { session, failures, phases, mic, lose } = setup();
    await session.start();
    lose();
    expect(failures[0][0]).toBe('The microphone stopped');
    expect(last(phases)).toBe('off');
    expect(mic.stop).toHaveBeenCalled();
  });

  it('stops quietly when something is not ready, leaving what said so to stand', async () => {
    const openMic = vi.fn();
    const { session, failures, phases } = setup({ ready: async () => false, openMic });
    await session.start();
    expect(openMic).not.toHaveBeenCalled();
    expect(failures).toEqual([]);
    expect(last(phases)).toBe('off');
  });

  it('turns a failed readiness check into a message', async () => {
    const { session, failures } = setup({ ready: async () => { throw new Error('IPC down.'); } });
    await session.start();
    expect(failures).toEqual([["Couldn't start the voice conversation", 'IPC down.']]);
  });

  it('closes a mic that opens after the press was taken back', async () => {
    const opening = deferred<{ stop: () => void }>();
    const late = { stop: vi.fn() };
    const { session, phases } = setup({ openMic: () => opening.promise });
    const starting = session.start();
    await Promise.resolve();
    session.stop();
    opening.resolve(late);
    await starting;

    expect(late.stop).toHaveBeenCalled();
    expect(phases).toEqual(['starting', 'off']);
  });

  it('does not open the mic if the press is taken back during the check', async () => {
    const check = deferred<boolean>();
    const openMic = vi.fn();
    const { session } = setup({ ready: () => check.promise, openMic });
    const starting = session.start();
    session.stop();
    check.resolve(true);
    await starting;
    expect(openMic).not.toHaveBeenCalled();
  });

  it('says nothing when a mic failure arrives after the press was taken back', async () => {
    const opening = deferred<{ stop: () => void }>();
    const { session, failures } = setup({ openMic: () => opening.promise });
    const starting = session.start();
    await Promise.resolve();
    session.stop();
    opening.reject(new MicError('denied'));
    await starting;
    expect(failures).toEqual([]);
  });

  it('stops everything when ended mid-reply, leaving the turn to finish in the chat', async () => {
    const { session, deps, mic, phases, turn, heard, speak } = setup();
    await session.start();
    await speak();
    session.replyGrew('One here. Two');
    await vi.waitFor(() => expect(heard[0]).toEqual(['One here.']));

    session.stop();

    expect(mic.stop).toHaveBeenCalled();
    expect(deps.stopSpeaking).toHaveBeenCalled();
    expect(deps.interruptTurn).not.toHaveBeenCalled();
    expect(last(phases)).toBe('off');

    turn.outcome.resolve('answered');
    await Promise.resolve();
    expect(last(phases)).toBe('off');
  });

  it('drops a transcript that arrives after the session ended', async () => {
    const text = deferred<string>();
    const { session, deps, frames } = setup({ transcribe: () => text.promise });
    await session.start();
    frames(QUIET, 5);
    frames(LOUD, 20);
    frames(QUIET, 40);
    session.stop();
    text.resolve('Too late.');
    await Promise.resolve();
    expect(deps.send).not.toHaveBeenCalled();
  });

  it('ignores the mic while working out what was said', async () => {
    const text = deferred<string>();
    const { session, deps, phases, frames } = setup({ transcribe: vi.fn(() => text.promise) });
    await session.start();
    frames(QUIET, 5);
    frames(LOUD, 20);
    frames(QUIET, 40);
    frames(LOUD, 40);
    expect(deps.transcribe).toHaveBeenCalledTimes(1);
    expect(last(phases)).toBe('transcribing');
  });

  it('can be stopped twice', () => {
    const { session, phases } = setup();
    session.stop();
    expect(phases).toEqual([]);
  });

  it('ignores reply text outside a turn', async () => {
    const { session, deps } = setup();
    await session.start();
    session.replyGrew('One here. Two here.');
    expect(deps.speak).not.toHaveBeenCalled();
  });
});
