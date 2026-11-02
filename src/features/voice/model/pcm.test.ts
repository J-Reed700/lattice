import { describe, expect, it } from 'vitest';

import { encodePcm } from './pcm';

function decode(pcm: string): number[] {
  const bytes = Uint8Array.from(atob(pcm), (c) => c.charCodeAt(0));
  const view = new DataView(bytes.buffer);
  return Array.from({ length: bytes.length / 2 }, (_, i) => view.getInt16(i * 2, true));
}

describe('encodePcm', () => {
  it('writes little-endian 16-bit samples', () => {
    expect(decode(encodePcm(Float32Array.of(0, 0.5, -0.5, 1, -1)))).toEqual([0, 16384, -16383, 32767, -32767]);
  });

  it('clips rather than wrapping a sample past full scale', () => {
    expect(decode(encodePcm(Float32Array.of(1.5, -1.5)))).toEqual([32767, -32767]);
  });

  it('carries a two-minute recording in one go', () => {
    const samples = new Float32Array(16_000 * 120).fill(0.25);
    expect(decode(encodePcm(samples))).toHaveLength(samples.length);
  });
});
