import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { FRAME_SAMPLES, MIC_CAPTURE_PROCESSOR } from './micProcessor';

type Registered = new () => { port: MessagePort; process(inputs: Float32Array[][]): boolean };

class FakeProcessor {
  port = { postMessage: vi.fn() } as unknown as MessagePort;
}

const posted = (processor: InstanceType<Registered>) =>
  vi.mocked(processor.port.postMessage).mock.calls.map(([frame]) => frame as Float32Array);

async function loadProcessor(rate: number): Promise<{ name: string; processor: InstanceType<Registered> }> {
  let name = '';
  let registered: Registered | null = null;
  vi.stubGlobal('sampleRate', rate);
  vi.stubGlobal('AudioWorkletProcessor', FakeProcessor);
  vi.stubGlobal('registerProcessor', (given: string, processor: Registered) => {
    name = given;
    registered = processor;
  });
  await import('./micCapture.worklet');
  if (!registered) throw new Error('nothing registered');
  return { name, processor: new (registered as Registered)() };
}

describe('the mic capture worklet', () => {
  beforeEach(() => {
    vi.resetModules();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('registers under the name the page asks for', async () => {
    const { name } = await loadProcessor(48_000);
    expect(name).toBe(MIC_CAPTURE_PROCESSOR);
  });

  it('turns 128-sample blocks at 48 kHz into 30 ms frames at 16 kHz', async () => {
    const { processor } = await loadProcessor(48_000);

    for (let block = 0; block < 60; block += 1) {
      expect(processor.process([[new Float32Array(128).fill(0.25)]])).toBe(true);
    }

    const frames = posted(processor);
    expect(frames.length).toBeGreaterThan(1);
    expect(frames.every((frame) => frame.length === FRAME_SAMPLES)).toBe(true);
    expect(frames.at(-1)?.[FRAME_SAMPLES - 1]).toBeCloseTo(0.25, 2);
  });

  it('hands each frame over rather than copying it, and starts a fresh one', async () => {
    const { processor } = await loadProcessor(16_000);

    for (let block = 0; block < 20; block += 1) processor.process([[new Float32Array(128)]]);

    const calls = vi.mocked(processor.port.postMessage).mock.calls;
    const [frame, transfer] = calls[0] as unknown as [Float32Array, ArrayBuffer[]];
    expect(transfer).toEqual([frame.buffer]);
    expect((calls[1] as unknown as [Float32Array])[0]).not.toBe(frame);
  });

  it('mixes stereo down to one channel', async () => {
    const { processor } = await loadProcessor(16_000);

    for (let block = 0; block < 20; block += 1) {
      processor.process([[new Float32Array(128).fill(0.5), new Float32Array(128).fill(-0.1)]]);
    }

    expect(posted(processor).at(-1)?.at(-1)).toBeCloseTo(0.2, 2);
  });

  it('keeps running through a block with no input', async () => {
    const { processor } = await loadProcessor(48_000);
    expect(processor.process([[]])).toBe(true);
    expect(processor.process([])).toBe(true);
    expect(posted(processor)).toHaveLength(0);
  });
});
