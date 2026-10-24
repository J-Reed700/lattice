import { create } from 'zustand';

import type { SourceWithMetadata } from '@/types/conversation';

export const READER_MIN_WIDTH = 380;
export const READER_MAX_WIDTH = 900;
export const READER_DEFAULT_WIDTH = 560;
export const READER_MIN_CHAT_WIDTH = 560;

const WIDTH_KEY = 'chat.reader.width';
export const MAX_RESOLVED_LOCATIONS = 512;

export interface ReaderSession {
  ownerKey: string;
  citations: SourceWithMetadata[];
  index: number;
  occurrence: number | null;
}

export interface ChatReaderState {
  session: ReaderSession | null;
  width: number;
  resolvedLocations: Map<string, string>;
  open: (
    _ownerKey: string,
    _citations: SourceWithMetadata[],
    _index: number,
    _occurrence?: number | null
  ) => void;
  setIndex: (_index: number) => void;
  close: () => void;
  setWidth: (_px: number) => void;
  rememberResolvedLocation: (_chunkId: string, _label: string) => void;
}

const clampWidth = (px: number): number =>
  Math.round(Math.min(READER_MAX_WIDTH, Math.max(READER_MIN_WIDTH, px)));

function readStoredWidth(): number {
  try {
    const stored = Number(localStorage.getItem(WIDTH_KEY));
    return Number.isFinite(stored) && stored > 0 ? clampWidth(stored) : READER_DEFAULT_WIDTH;
  } catch {
    return READER_DEFAULT_WIDTH;
  }
}

function writeStoredWidth(px: number): void {
  try {
    localStorage.setItem(WIDTH_KEY, String(px));
  } catch {
    // Preference only.
  }
}

export const useChatReaderStore = create<ChatReaderState>((set, get) => ({
  session: null,
  width: readStoredWidth(),
  resolvedLocations: new Map(),

  open: (ownerKey, citations, index, occurrence = null) => {
    if (citations.length === 0) return;
    set({
      session: {
        ownerKey,
        citations,
        index: Math.min(citations.length - 1, Math.max(0, index)),
        occurrence,
      },
    });
  },

  setIndex: (index) =>
    set((state) => {
      const session = state.session;
      if (!session) return {};
      const next = Math.min(session.citations.length - 1, Math.max(0, index));
      if (next === session.index) return {};
      return { session: { ...session, index: next, occurrence: null } };
    }),

  close: () => {
    if (!get().session) return;
    set({ session: null });
  },

  setWidth: (px) => {
    const width = clampWidth(px);
    if (width === get().width) return;
    writeStoredWidth(width);
    set({ width });
  },

  rememberResolvedLocation: (chunkId, label) => {
    if (!chunkId || !label) return;
    set((state) => {
      if (state.resolvedLocations.get(chunkId) === label) return {};
      const resolvedLocations = new Map(state.resolvedLocations);
      resolvedLocations.delete(chunkId);
      resolvedLocations.set(chunkId, label);
      for (const oldest of resolvedLocations.keys()) {
        if (resolvedLocations.size <= MAX_RESOLVED_LOCATIONS) break;
        resolvedLocations.delete(oldest);
      }
      return { resolvedLocations };
    });
  },
}));
