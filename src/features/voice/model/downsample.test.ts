import { describe, expect, it } from 'vitest';

import { Downsampler, SPEECH_SAMPLE_RATE } from './downsample';

function sine(hz: number, rate: number, seconds: number): Float32Array {
  return Float32Array.from({ length: Math.round(rate * seconds) }, (_, i) => Math.sin((2 * Math.PI * hz * i) / rate));
}

function rms(samples: Float32Array): number {
  return Math.sqrt(samples.reduce((sum, value) => sum + value * value, 0) / samples.length);
}

/** Past the filter's start-up and its zero-padded edge. */
function settled(samples: Float32Array): Float32Array {
  return samples.slice(800, -800);
}

/** Zero crossings a second, which is twice the frequency of a pure tone. */
function frequency(samples: Float32Array, rate: number): number {
  let crossings = 0;
  for (let i = 1; i < samples.length; i += 1) {
    if (samples[i - 1] < 0 !== samples[i] < 0) crossings += 1;
  }
  return (crossings * rate) / (2 * samples.length);
}

function inBlocks(downsampler: Downsampler, samples: Float32Array, size: number): Float32Array {
  const out: number[] = [];
  for (let at = 0; at < samples.length; at += size) {
    out.push(...downsampler.process(samples.subarray(at, at + size)));
  }
  return Float32Array.from(out);
}

describe('Downsampler', () => {
  it.each([48_000, 44_100, 96_000])('makes a second of %i Hz audio a second at 16 kHz', (rate) => {
    const output = new Downsampler(rate).process(sine(440, rate, 1));
    expect(Math.abs(output.length - SPEECH_SAMPLE_RATE)).toBeLessThan(150);
  });

  it.each([48_000, 44_100])('keeps a voice-band tone from %i Hz at its pitch and level', (rate) => {
    const output = settled(new Downsampler(rate).process(sine(1_000, rate, 1)));
    expect(rms(output)).toBeCloseTo(Math.SQRT1_2, 1);
    expect(frequency(output, SPEECH_SAMPLE_RATE)).toBeCloseTo(1_000, -1);
  });

  it('removes what would fold back below 8 kHz rather than aliasing it', () => {
    const rate = 48_000;
    const output = settled(new Downsampler(rate).process(sine(12_000, rate, 1)));
    expect(20 * Math.log10(rms(output) / Math.SQRT1_2)).toBeLessThan(-30);
  });

  it('gives the same samples however the input is cut into blocks', () => {
    const rate = 44_100;
    const input = sine(700, rate, 0.25);
    const whole = new Downsampler(rate).process(input);
    const blocks = inBlocks(new Downsampler(rate), input, 128);
    expect(blocks.length).toBe(whole.length);
    blocks.forEach((value, i) => expect(value).toBeCloseTo(whole[i], 5));
  });

  it('holds a block shorter than the filter rather than emitting noise', () => {
    const downsampler = new Downsampler(48_000);
    expect(downsampler.process(new Float32Array(2)).length).toBe(0);
  });

  it('passes 16 kHz through at its own rate', () => {
    const output = settled(new Downsampler(SPEECH_SAMPLE_RATE).process(sine(1_000, SPEECH_SAMPLE_RATE, 1)));
    expect(frequency(output, SPEECH_SAMPLE_RATE)).toBeCloseTo(1_000, -1);
  });
});
