import { describe, expect, it } from 'vitest';

import { FRAME_SAMPLES } from './micProcessor';
import { SpeechDetector, type SpeechEvent } from './speechDetector';

const SILENCE_DB = -70;
const VOICE_DB = -30;

/** A frame at `db` dBFS, marked so a test can find it in the utterance. */
function frame(db: number): Float32Array {
  const amplitude = 10 ** (db / 20);
  return Float32Array.from({ length: FRAME_SAMPLES }, (_, i) => (i % 2 ? amplitude : -amplitude));
}

function feed(detector: SpeechDetector, count: number, db: number, appSpeaking = false): SpeechEvent[] {
  const events: SpeechEvent[] = [];
  for (let i = 0; i < count; i += 1) {
    const event = detector.push(frame(db), appSpeaking);
    if (event) events.push(event);
    if (event === 'ended') break;
  }
  return events;
}

describe('SpeechDetector', () => {
  it('stays quiet in a quiet room', () => {
    expect(feed(new SpeechDetector(), 200, SILENCE_DB)).toEqual([]);
  });

  it('starts on sustained speech and ends after a pause', () => {
    const detector = new SpeechDetector();
    feed(detector, 30, SILENCE_DB);
    expect(feed(detector, 20, VOICE_DB)).toEqual(['started']);
    expect(feed(detector, 40, SILENCE_DB)).toEqual(['ended']);
  });

  it('ignores a click shorter than a syllable', () => {
    const detector = new SpeechDetector();
    feed(detector, 30, SILENCE_DB);
    expect(feed(detector, 3, VOICE_DB)).toEqual([]);
    expect(feed(detector, 30, SILENCE_DB)).toEqual([]);
  });

  it('does not end at a breath in the middle of a sentence', () => {
    const detector = new SpeechDetector();
    feed(detector, 30, SILENCE_DB);
    feed(detector, 20, VOICE_DB);
    expect(feed(detector, 15, SILENCE_DB)).toEqual([]);
    expect(feed(detector, 20, VOICE_DB)).toEqual([]);
    expect(feed(detector, 40, SILENCE_DB)).toEqual(['ended']);
  });

  it('keeps the audio from just before the start, so a first word is not clipped', () => {
    const detector = new SpeechDetector();
    feed(detector, 30, SILENCE_DB);
    feed(detector, 20, VOICE_DB);
    feed(detector, 40, SILENCE_DB);

    const speech = detector.take();
    const seconds = speech.length / 16_000;
    expect(seconds).toBeGreaterThan(1.5);
    // The frames before the first loud one came along: the audio opens quiet.
    expect(Math.abs(speech[0])).toBeLessThan(0.001);
  });

  it('ends a very long speech rather than recording forever', () => {
    const detector = new SpeechDetector();
    feed(detector, 30, SILENCE_DB);
    feed(detector, 20, VOICE_DB);
    expect(feed(detector, 4_100, VOICE_DB)).toEqual(['ended']);
  });

  it('takes a steady noise for the room rather than for someone speaking', () => {
    const detector = new SpeechDetector();
    feed(detector, 300, -52);
    expect(feed(detector, 60, -45)).toEqual([]);
  });

  it('is not fooled by a room louder than its floor from the very first frame', () => {
    const detector = new SpeechDetector();
    expect(feed(detector, 300, -40)).toEqual([]);
    expect(feed(detector, 20, -20)).toEqual(['started']);
  });

  it('gets used to a noise that starts up and stays', () => {
    const detector = new SpeechDetector();
    feed(detector, 100, SILENCE_DB);
    feed(detector, 20, -35);
    expect(feed(detector, 200, -35)).toEqual(['ended']);
    detector.take();
    expect(feed(detector, 200, -35)).toEqual([]);
  });

  it('still hears a voice over that noise', () => {
    const detector = new SpeechDetector();
    feed(detector, 150, -40);
    expect(feed(detector, 20, -22)).toEqual(['started']);
  });

  it('needs more than the app’s own echo before it counts as someone speaking over it', () => {
    const detector = new SpeechDetector();
    feed(detector, 30, SILENCE_DB, true);
    // The residual echo measured with the canceller on stays around -53 dB at peak.
    expect(feed(detector, 40, -53, true)).toEqual([]);
    expect(feed(detector, 40, -45, true)).toEqual([]);
    expect(feed(detector, 20, VOICE_DB, true)).toEqual(['started']);
  });

  it('takes longer to decide while the app is speaking', () => {
    const quiet = new SpeechDetector();
    const speaking = new SpeechDetector();
    feed(quiet, 30, SILENCE_DB);
    feed(speaking, 30, SILENCE_DB, true);
    expect(feed(quiet, 8, VOICE_DB)).toEqual(['started']);
    expect(feed(speaking, 8, VOICE_DB, true)).toEqual([]);
  });

  it('starts over after take and reset', () => {
    const detector = new SpeechDetector();
    feed(detector, 5, SILENCE_DB);
    feed(detector, 20, VOICE_DB);
    expect(detector.take().length).toBeGreaterThan(0);
    expect(detector.take().length).toBe(0);
    feed(detector, 20, VOICE_DB);
    detector.reset();
    expect(detector.take().length).toBe(0);
  });
});
