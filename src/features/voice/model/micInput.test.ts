import { describe, expect, it, vi } from 'vitest';

import { MicError, openMic } from './micInput';

function namedError(name: string): Error {
  const error = new Error('browser wording');
  error.name = name;
  return error;
}

function fakes({ state = 'running', addModule = vi.fn(async () => undefined) } = {}) {
  const track = { stop: vi.fn(), onended: null as (() => void) | null };
  const stream = { getAudioTracks: () => [track] } as unknown as MediaStream;
  const source = { connect: vi.fn(), disconnect: vi.fn() };
  const capture = {
    port: { onmessage: null as ((event: MessageEvent<Float32Array>) => void) | null },
    connect: vi.fn(),
    disconnect: vi.fn(),
  };
  const context = {
    state,
    audioWorklet: { addModule },
    destination: {},
    resume: vi.fn(async () => undefined),
    close: vi.fn(async () => undefined),
    createMediaStreamSource: vi.fn(() => source),
  };
  const getUserMedia = vi.fn(async () => stream);
  return {
    track,
    source,
    capture,
    context,
    getUserMedia,
    environment: {
      media: { getUserMedia },
      createContext: () => context as unknown as AudioContext,
      createCapture: () => capture as unknown as AudioWorkletNode,
    },
  };
}

const handlers = () => ({ onFrame: vi.fn(), onLost: vi.fn() });

describe('openMic', () => {
  it('asks for a voice-ready mic and delivers the worklet’s frames', async () => {
    const { environment, getUserMedia, capture, source, context } = fakes();
    const on = handlers();
    await openMic(on, environment);

    expect(getUserMedia).toHaveBeenCalledWith(expect.objectContaining({
      audio: expect.objectContaining({ echoCancellation: true, noiseSuppression: true, autoGainControl: true }),
    }));
    expect(source.connect).toHaveBeenCalledWith(capture);
    expect(capture.connect).toHaveBeenCalledWith(context.destination);

    const frame = new Float32Array(480);
    capture.port.onmessage?.({ data: frame } as MessageEvent<Float32Array>);
    expect(on.onFrame).toHaveBeenCalledWith(frame);
  });

  it('wakes a context the browser started suspended', async () => {
    const { environment, context } = fakes({ state: 'suspended' });
    await openMic(handlers(), environment);
    expect(context.resume).toHaveBeenCalled();
  });

  it('lets the mic go when stopped, so the system’s indicator goes off', async () => {
    const { environment, track, context, capture, source } = fakes();
    const mic = await openMic(handlers(), environment);
    mic.stop();

    expect(track.stop).toHaveBeenCalled();
    expect(context.close).toHaveBeenCalled();
    expect(capture.port.onmessage).toBeNull();
    expect(source.disconnect).toHaveBeenCalled();
  });

  it('says so when the mic is pulled out', async () => {
    const { environment, track } = fakes();
    const on = handlers();
    await openMic(on, environment);
    track.onended?.();
    expect(on.onLost).toHaveBeenCalled();
  });

  it.each([
    ['NotAllowedError', 'denied'],
    ['SecurityError', 'denied'],
    ['NotFoundError', 'missing'],
    ['OverconstrainedError', 'missing'],
    ['NotReadableError', 'busy'],
    ['AbortError', 'busy'],
    ['TypeError', 'failed'],
  ])('turns %s into a failure the reader can act on', async (name, failure) => {
    const { environment, getUserMedia } = fakes();
    getUserMedia.mockRejectedValueOnce(namedError(name));

    const error = await openMic(handlers(), environment).catch((caught: unknown) => caught);

    expect(error).toBeInstanceOf(MicError);
    expect((error as MicError).failure).toBe(failure);
    expect((error as MicError).message).not.toContain('browser wording');
    expect((error as MicError).message.length).toBeGreaterThan(20);
  });

  it('points a denied mic at the system’s setting', async () => {
    const { environment, getUserMedia } = fakes();
    getUserMedia.mockRejectedValueOnce(namedError('NotAllowedError'));
    await expect(openMic(handlers(), environment)).rejects.toThrow(/Privacy & security/);
  });

  it('says so where the window has no way to reach a mic', async () => {
    const { environment } = fakes();
    await expect(openMic(handlers(), { ...environment, media: undefined })).rejects.toThrow(/can.t reach a microphone/);
  });

  it('closes the mic again if the audio graph can’t be built', async () => {
    const { environment, track, context } = fakes({ addModule: vi.fn(async () => { throw new Error('worklet failed'); }) });

    await expect(openMic(handlers(), environment)).rejects.toBeInstanceOf(MicError);
    expect(track.stop).toHaveBeenCalled();
    expect(context.close).toHaveBeenCalled();
  });
});
