import {
  byOccurrence,
  citing,
  closeGaps,
  sentencesOf,
  type ReferenceReader,
} from '@/features/reading/model/answerSentences';
import type { FoundMark } from '@/utils/foundMark';

/**
 * The notes format's own citation marker: `[n]`, written by the app when an
 * answer is saved into a note and typed by the user. It is read here and
 * nowhere else; a conversation's reply never goes through this module, its
 * references are pieces.
 */
const NOTE_MARKER = /\[(\d{1,5})\]/g;

export function findNoteMarks(text: string): FoundMark[] {
  return Array.from(text.matchAll(NOTE_MARKER), (match) => ({
    index: match.index ?? 0,
    length: match[0].length,
    number: Number(match[1]),
    glyph: false,
  }));
}

const noteReader: ReferenceReader = {
  numbersIn: (sentence) => findNoteMarks(sentence).map((mark) => mark.number),
  withoutReferences: (sentence) => closeGaps(sentence.replace(NOTE_MARKER, ' ')),
};

/** The sentences of a note that cite source `citationNumber`, markers taken out. */
export function noteSentencesCiting(note: string, citationNumber: number): string[] {
  return citing(sentencesOf(note, noteReader), citationNumber);
}

/** Entry `k` is the sentence holding the k-th `[n]` of source `citationNumber` in the note. */
export function noteSentencesByOccurrence(note: string, citationNumber: number): string[] {
  return byOccurrence(sentencesOf(note, noteReader), citationNumber);
}
