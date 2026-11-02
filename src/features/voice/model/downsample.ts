/** What Whisper listens at. */
export const SPEECH_SAMPLE_RATE = 16_000;

/** Fractional positions the filter is worked out for ahead of time. */
const PHASES = 64;
/** Sinc lobes kept on each side of a sample: more is a steeper filter. */
const LOBES = 8;

function sinc(x: number): number {
  return x === 0 ? 1 : Math.sin(Math.PI * x) / (Math.PI * x);
}

/** A Hann-windowed low-pass centred `fraction` of a sample past its middle. */
function lowPass(fraction: number, half: number, cutoff: number): Float32Array {
  const weights = new Float32Array(half * 2);
  let sum = 0;
  for (let tap = 0; tap < weights.length; tap += 1) {
    const x = tap - half + 1 - fraction;
    const weight = sinc(cutoff * x) * (0.5 + 0.5 * Math.cos((Math.PI * x) / half));
    weights[tap] = weight;
    sum += weight;
  }
  return weights.map((weight) => weight / sum);
}

/**
 * Turns a stream of samples into a lower rate, one block at a time. Whatever
 * sits above the new rate's Nyquist is filtered out first, or it would fold
 * back down as noise. A block's cut falls anywhere: the samples a later output
 * still needs are kept for the next call.
 */
export class Downsampler {
  private readonly step: number;
  private readonly half: number;
  private readonly filters: Float32Array[];
  private kept: Float32Array;
  /** The next output's position in `kept`, in input samples. */
  private position: number;

  constructor(inputRate: number, outputRate = SPEECH_SAMPLE_RATE) {
    this.step = inputRate / outputRate;
    const cutoff = Math.min(1, 1 / this.step) * 0.9;
    this.half = Math.ceil(LOBES / cutoff);
    this.filters = Array.from({ length: PHASES + 1 }, (_, phase) => lowPass(phase / PHASES, this.half, cutoff));
    this.kept = new Float32Array(this.half);
    this.position = this.half;
  }

  process(input: Float32Array): Float32Array {
    const samples = new Float32Array(this.kept.length + input.length);
    samples.set(this.kept);
    samples.set(input, this.kept.length);

    const room = Math.max(0, Math.ceil((samples.length - this.position) / this.step));
    const output = new Float32Array(room);
    let written = 0;
    let position = this.position;
    while (position + this.half < samples.length) {
      const index = Math.floor(position);
      const filter = this.filters[Math.round((position - index) * PHASES)];
      const first = index - this.half + 1;
      let sum = 0;
      for (let tap = 0; tap < filter.length; tap += 1) sum += filter[tap] * samples[first + tap];
      output[written] = sum;
      written += 1;
      position += this.step;
    }

    const consumed = Math.min(samples.length, Math.max(0, Math.floor(position) - this.half + 1));
    this.kept = samples.slice(consumed);
    this.position = position - consumed;
    return output.slice(0, written);
  }
}
