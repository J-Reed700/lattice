import type { FoundMark } from '@/utils/foundMark';

/**
 * The notes format's own citation marker: `[n]`, written by the app when an
 * answer is saved into a note and typed by the user. It is read here and
 * nowhere else; a conversation's reply never goes through this module, its
 * references are pieces.
 */
export const NOTE_MARKER = /\[(\d{1,5})\]/g;

export function findNoteMarks(text: string): FoundMark[] {
  return Array.from(text.matchAll(NOTE_MARKER), (match) => ({
    index: match.index ?? 0,
    length: match[0].length,
    number: Number(match[1]),
    glyph: false,
  }));
}
