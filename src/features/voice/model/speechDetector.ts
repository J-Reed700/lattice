import { SPEECH_SAMPLE_RATE } from './downsample';
import { FRAME_SAMPLES } from './micProcessor';

const FRAME_MS = (FRAME_SAMPLES / SPEECH_SAMPLE_RATE) * 1000;
const frames = (ms: number) => Math.ceil(ms / FRAME_MS);

/** The quietest that counts as a voice, however still the room is. */
const MIN_LEVEL_DB = -50;
/** While the app itself is speaking: its echo was measured peaking near -53 dB with the canceller on. */
const MIN_LEVEL_WHILE_SPEAKING_DB = -40;
const ABOVE_ROOM_DB = 12;
/** The room is the quietest moment this far back: speech has gaps, a fan doesn't. */
const ROOM_MS = 3_000;
const START_MS = 210;
const START_WHILE_SPEAKING_MS = 300;
const PAUSE_MS = 1_000;
const LEAD_IN_MS = 450;
const LONGEST_MS = 120_000;

function levelDb(frame: Float32Array): number {
  let sum = 0;
  for (const sample of frame) sum += sample * sample;
  return 10 * Math.log10(sum / frame.length + 1e-12);
}

export type SpeechEvent = 'started' | 'ended';

/**
 * Finds where someone starts and stops speaking in the microphone's frames, by
 * loudness against the room's own level. It keeps a little audio from before
 * each start, since the first syllable is what sets it off.
 */
export class SpeechDetector {
  private levels: number[] = [];
  private recent: Float32Array[] = [];
  private utterance: Float32Array[] | null = null;
  private loudFrames = 0;
  private quietFrames = 0;

  /** `appSpeaking` asks for more before calling it a voice: the app's own echo is about. */
  push(frame: Float32Array, appSpeaking = false): SpeechEvent | null {
    this.levels.push(levelDb(frame));
    if (this.levels.length > frames(ROOM_MS)) this.levels.shift();
    const floor = appSpeaking ? MIN_LEVEL_WHILE_SPEAKING_DB : MIN_LEVEL_DB;
    const room = Math.min(...this.levels);
    const loud = this.levels[this.levels.length - 1] > Math.max(floor, room + ABOVE_ROOM_DB);

    if (this.utterance) {
      this.utterance.push(frame);
      this.quietFrames = loud ? 0 : this.quietFrames + 1;
      const ended = this.quietFrames >= frames(PAUSE_MS) || this.utterance.length >= frames(LONGEST_MS);
      return ended ? 'ended' : null;
    }

    this.recent.push(frame);
    if (this.recent.length > frames(LEAD_IN_MS)) this.recent.shift();
    this.loudFrames = loud ? this.loudFrames + 1 : Math.max(0, this.loudFrames - 1);

    if (this.loudFrames < frames(appSpeaking ? START_WHILE_SPEAKING_MS : START_MS)) return null;
    this.utterance = this.recent;
    this.recent = [];
    this.quietFrames = 0;
    return 'started';
  }

  /** The audio from the last start to now, as one buffer; the detector starts over. */
  take(): Float32Array {
    const pieces = this.utterance ?? [];
    const samples = new Float32Array(pieces.reduce((total, piece) => total + piece.length, 0));
    let at = 0;
    for (const piece of pieces) {
      samples.set(piece, at);
      at += piece.length;
    }
    this.reset();
    return samples;
  }

  /** Forgets any speech in progress; what it has learned of the room stays. */
  reset() {
    this.recent = [];
    this.utterance = null;
    this.loudFrames = 0;
    this.quietFrames = 0;
  }
}
