import { create } from 'zustand';

/**
 * Read-aloud's UI-only preferences: the voice and its pace. They survive a
 * restart; the backend never needs them.
 */

export const DEFAULT_VOICE = 'bf_emma';
export const SPEEDS = [0.8, 0.9, 1, 1.1, 1.25, 1.5] as const;

const VOICE_KEY = 'speech.voice';
const SPEED_KEY = 'speech.speed';

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Preference only.
  }
}

function readSpeed(): number {
  const speed = Number(read(SPEED_KEY));
  return SPEEDS.some((allowed) => allowed === speed) ? speed : 1;
}

interface SpeechPrefsState {
  voice: string;
  speed: number;
  setVoice: (voice: string) => void;
  setSpeed: (speed: number) => void;
}

export const useSpeechPrefsStore = create<SpeechPrefsState>((set) => ({
  voice: read(VOICE_KEY) ?? DEFAULT_VOICE,
  speed: readSpeed(),
  setVoice: (voice) => {
    write(VOICE_KEY, voice);
    set({ voice });
  },
  setSpeed: (speed) => {
    write(SPEED_KEY, String(speed));
    set({ speed });
  },
}));
