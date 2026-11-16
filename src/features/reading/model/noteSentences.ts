import {
  byOccurrence,
  citing,
  closeGaps,
  sentencesOf,
  withoutInlineCode,
  type ReferenceReader,
} from '@/features/reading/model/answerSentences';
import { findNoteMarks, NOTE_MARKER } from '@/utils/noteCitations';

const noteReader: ReferenceReader = {
  numbersIn: (sentence) => findNoteMarks(sentence).map((mark) => mark.number),
  withoutReferences: (sentence) => closeGaps(sentence.replace(NOTE_MARKER, ' ')),
};

/** The sentences of a note that cite source `citationNumber`, markers taken out. */
export function noteSentencesCiting(note: string, citationNumber: number): string[] {
  return citing(sentencesOf(note, noteReader), citationNumber);
}

/**
 * Entry `k` is the sentence holding the k-th `[n]` chip of source `citationNumber` in the note. A chip is
 * never drawn inside code, so a `[n]` there is not counted.
 */
export function noteSentencesByOccurrence(note: string, citationNumber: number): string[] {
  return byOccurrence(sentencesOf(withoutInlineCode(note), noteReader), citationNumber);
}
