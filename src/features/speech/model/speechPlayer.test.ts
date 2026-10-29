import { describe, expect, it, vi } from 'vitest';

import { decodeClip, SpeechPlayer, type AudioOutput, type SpeechClip } from './speechPlayer';

class FakeNode {
  buffer: { duration: number } | null = null;
  onended: (() => void) | null = null;
  startedAt: number | null = null;
  stopped = false;
  connect = vi.fn();
  start(at: number) {
    this.startedAt = at;
  }
  stop() {
    this.stopped = true;
  }
}

class FakeOutput {
  currentTime = 0;
  state = 'running';
  nodes: FakeNode[] = [];
  destination = {} as AudioNode;
  resume = vi.fn(async () => {
    this.state = 'running';
  });
  suspend = vi.fn(async () => {
    this.state = 'suspended';
  });
  createBuffer(_channels: number, length: number, sampleRate: number) {
    return { duration: length / sampleRate, copyToChannel: vi.fn() } as unknown as AudioBuffer;
  }
  createBufferSource() {
    const node = new FakeNode();
    this.nodes.push(node);
    return node as unknown as AudioBufferSourceNode;
  }
}

/** A clip of `seconds` of silence at 10 samples a second. */
function clip(seconds: number): SpeechClip {
  const bytes = new Uint8Array(seconds * 10 * 2);
  return { sampleRate: 10, pcm: btoa(String.fromCharCode(...bytes)), durationMs: seconds * 1000 };
}

function setup() {
  const output = new FakeOutput();
  const player = new SpeechPlayer(() => output as unknown as AudioOutput);
  return { output, player };
}

describe('SpeechPlayer', () => {
  it('schedules each piece to start when the one before it ends', async () => {
    const { output, player } = setup();
    const synthesize = vi.fn(async (text: string) => clip(text.length));
    await player.play('answer', ['ab', 'abc', 'a'], synthesize);

    expect(synthesize.mock.calls.map(([text]) => text)).toEqual(['ab', 'abc', 'a']);
    expect(output.nodes.map((node) => node.startedAt)).toEqual([0, 2, 5]);
    expect(player.getSnapshot()).toEqual({ sourceId: 'answer', status: 'playing', error: null });

    output.nodes.at(-1)?.onended?.();
    expect(player.getSnapshot().status).toBe('idle');
  });

  it('plays one thing at a time: a new source stops the old one', async () => {
    const { output, player } = setup();
    await player.play('first', ['one'], async () => clip(3));
    const firstNode = output.nodes[0];

    await player.play('second', ['two'], async () => clip(1));
    expect(firstNode.stopped).toBe(true);
    expect(player.getSnapshot().sourceId).toBe('second');
  });

  it('stops before a piece still being made is played', async () => {
    const { output, player } = setup();
    let finish: (value: SpeechClip) => void = () => {};
    const pending = player.play('answer', ['one'], () => new Promise((resolve) => { finish = resolve; }));
    expect(player.getSnapshot().status).toBe('loading');

    player.stop('answer');
    finish(clip(1));
    await pending;
    expect(output.nodes).toHaveLength(0);
    expect(player.getSnapshot()).toEqual({ sourceId: null, status: 'idle', error: null });
  });

  it('pauses and resumes the output, and ignores a stop meant for another source', async () => {
    const { output, player } = setup();
    await player.play('answer', ['one'], async () => clip(2));

    player.pause();
    expect(output.suspend).toHaveBeenCalled();
    expect(player.getSnapshot().status).toBe('paused');

    player.stop('something-else');
    expect(player.getSnapshot().status).toBe('paused');

    player.resume();
    expect(output.resume).toHaveBeenCalled();
    expect(player.getSnapshot().status).toBe('playing');
  });

  it('reports a failed piece and goes quiet', async () => {
    const { player } = setup();
    await player.play('answer', ['one'], async () => {
      throw new Error('The read-aloud voices are not downloaded.');
    });
    expect(player.getSnapshot()).toEqual({
      sourceId: null,
      status: 'idle',
      error: 'The read-aloud voices are not downloaded.',
    });
  });
});

describe('decodeClip', () => {
  it('turns little-endian 16-bit PCM into floats', () => {
    const output = new FakeOutput();
    const buffer = output.createBuffer(1, 2, 24000);
    const create = vi.spyOn(output, 'createBuffer').mockReturnValue(buffer);
    const pcm = btoa(String.fromCharCode(0x00, 0x40, 0x00, 0xc0));
    decodeClip(output as unknown as AudioOutput, { sampleRate: 24000, pcm, durationMs: 0 });
    expect(create).toHaveBeenCalledWith(1, 2, 24000);
    expect(buffer.copyToChannel).toHaveBeenCalledWith(new Float32Array([0.5, -0.5]), 0);
  });
});
