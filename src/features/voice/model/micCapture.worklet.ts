import { Downsampler } from './downsample';
import { FRAME_SAMPLES, MIC_CAPTURE_PROCESSOR } from './micProcessor';

/** Globals of the audio thread, which the DOM library does not describe. */
declare const sampleRate: number;
declare function registerProcessor(name: string, processor: new () => AudioWorkletProcessor): void;
declare class AudioWorkletProcessor {
  readonly port: MessagePort;
  process(inputs: Float32Array[][]): boolean;
}

function mono(channels: Float32Array[]): Float32Array {
  if (channels.length === 1) return channels[0];
  const mixed = new Float32Array(channels[0].length);
  for (const channel of channels) {
    for (let i = 0; i < mixed.length; i += 1) mixed[i] += channel[i] / channels.length;
  }
  return mixed;
}

/** Runs on the audio thread: the microphone, mono and at 16 kHz, in whole frames. */
class MicCapture extends AudioWorkletProcessor {
  private readonly downsampler = new Downsampler(sampleRate);
  private frame = new Float32Array(FRAME_SAMPLES);
  private filled = 0;

  override process(inputs: Float32Array[][]): boolean {
    const channels = inputs[0];
    if (!channels || channels.length === 0) return true;
    for (const sample of this.downsampler.process(mono(channels))) {
      this.frame[this.filled] = sample;
      this.filled += 1;
      if (this.filled === FRAME_SAMPLES) {
        this.port.postMessage(this.frame, [this.frame.buffer]);
        this.frame = new Float32Array(FRAME_SAMPLES);
        this.filled = 0;
      }
    }
    return true;
  }
}

registerProcessor(MIC_CAPTURE_PROCESSOR, MicCapture);
