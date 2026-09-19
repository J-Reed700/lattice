/**
 * The source reader that sits beside the thread.
 *
 * One reader for the whole chat, not one per message: the reader is a third
 * pane, and a pane that existed once per answer could only ever be an overlay
 * on top of the answer it belonged to.
 *
 * `ownerKey` is the message the open citations came from. It is what lets an
 * answer light the citation the reader is showing without lighting the same
 * number in every other answer on screen.
 */

import { create } from 'zustand';

import type { SourceWithMetadata } from '../types/conversation';

/** Narrower than this and the reader is a column of broken lines. */
export const READER_MIN_WIDTH = 380;
/** Wider than this and the document is a strip of text in an empty field. */
export const READER_MAX_WIDTH = 900;
export const READER_DEFAULT_WIDTH = 560;
/**
 * The chat column the reader must leave standing.
 *
 * The same number decides both whether the reader may dock at all and how far
 * it may be dragged, so the answer never becomes the narrower of the two.
 */
export const READER_MIN_CHAT_WIDTH = 560;

const WIDTH_KEY = 'chat.reader.width';

export interface ReaderSession {
  /** The message these citations belong to. */
  ownerKey: string;
  citations: SourceWithMetadata[];
  index: number;
}

export interface ChatReaderState {
  session: ReaderSession | null;
  width: number;
  /**
   * Where a citation turned out to be, once a viewer resolved it (a PDF page,
   * a heading). Keyed by chunk id, so every answer citing that chunk says the
   * page, not just the one whose reader resolved it.
   */
  resolvedLocations: Map<string, string>;
  open: (_ownerKey: string, _citations: SourceWithMetadata[], _index: number) => void;
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

  open: (ownerKey, citations, index) => {
    if (citations.length === 0) return;
    set({
      session: {
        ownerKey,
        citations,
        index: Math.min(citations.length - 1, Math.max(0, index)),
      },
    });
  },

  setIndex: (index) =>
    set((state) => {
      const session = state.session;
      if (!session) return {};
      const next = Math.min(session.citations.length - 1, Math.max(0, index));
      if (next === session.index) return {};
      return { session: { ...session, index: next } };
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
      return { resolvedLocations: new Map(state.resolvedLocations).set(chunkId, label) };
    });
  },
}));
