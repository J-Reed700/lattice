/**
 * The sentences of an answer that cite a particular source.
 *
 * Used when a turn was never verified: with no claim verdicts to read, the only
 * record of which sentences leaned on source `n` is the references the answer
 * made, in its pieces.
 */

import { sentenceSpans } from '@/features/reading/model/sourcedPassages';
import { MARK, markIndex, markReply, QUOTE_END, QUOTE_START, type ReplyPiece } from '@/utils/replyPieces';

/** The marks a quotation travels between: the words matter to a matcher, the marks do not. */
const QUOTE_MARKS = new RegExp(`[${QUOTE_START}${QUOTE_END}]`, 'g');

/** Markdown that carries no words: list bullets, headings, quotes, table pipes. */
const BLOCK_PREFIX = /^\s*(?:[-*+]\s+|\d{1,3}[.)]\s+|#{1,6}\s+|>\s*|\|\s*)/;

/** The opening or closing line of a fenced code block. */
const FENCE = /^\s*(?:```|~~~)/;

/** Inline code, whose `[n]` a chip is never drawn over. */
const INLINE_CODE = /`[^`\n]*`/g;

/** `text` with its inline code blanked; a fence line is left so the splitter still sees the block open. */
export function withoutInlineCode(text: string): string {
  return text
    .split('\n')
    .map((line) => (FENCE.test(line) ? line : line.replace(INLINE_CODE, ' ')))
    .join('\n');
}

/** Emphasis markers, which are punctuation to a reader and noise to a matcher. */
const EMPHASIS = /(\*\*|__|\*|_|`)/g;

function cleanLine(line: string): string {
  return line
    .replace(BLOCK_PREFIX, '')
    .replace(/\s*\|\s*$/, '')
    .replace(EMPHASIS, '')
    .replace(/\s+/g, ' ')
    .trim();
}

/**
 * An answer split into sentences, one markdown line at a time.
 *
 * Line by line because an answer is a document: a heading, a bullet and a table
 * row are each their own sentence however they are punctuated.
 */
export function splitAnswerSentences(text: string): string[] {
  const sentences: string[] = [];
  let fenced = false;
  for (const line of text.split('\n')) {
    // Code is not something the answer said.
    if (FENCE.test(line)) {
      fenced = !fenced;
      continue;
    }
    if (fenced) continue;
    const cleaned = cleanLine(line);
    if (!cleaned) continue;
    for (const span of sentenceSpans(cleaned)) {
      const sentence = cleaned.slice(span.start, span.end).trim();
      if (sentence) sentences.push(sentence);
    }
  }
  return sentences;
}

/** One sentence of an answer: its words, and the source numbers its references cite in the order made. */
interface AnswerSentence {
  text: string;
  numbers: number[];
}

/** How one format of references is found in a sentence and taken out of it. */
export interface ReferenceReader {
  numbersIn: (sentence: string) => number[];
  /** The sentence with the references removed, for a matcher or a tooltip. */
  withoutReferences: (sentence: string) => string;
}

/** A cited sentence as a matcher and a tooltip want it: no marks, no gaps. */
export function closeGaps(sentence: string): string {
  return (
    sentence
      .replace(/\s+/g, ' ')
      // The reference sat before the full stop; the gap it leaves would otherwise
      // show up in the tooltip that names the sentence.
      .replace(/\s+([.,;:!?])/g, '$1')
      .trim()
  );
}

export function sentencesOf(raw: string, reader: ReferenceReader): AnswerSentence[] {
  return splitAnswerSentences(raw).map((sentence) => ({
    text: reader.withoutReferences(sentence),
    numbers: reader.numbersIn(sentence),
  }));
}

function replyReader(numbers: readonly number[]): ReferenceReader {
  return {
    numbersIn: (sentence) =>
      Array.from(sentence.matchAll(MARK), (match) => numbers[markIndex(match[1])]).filter(
        (number): number is number => number !== undefined
      ),
    withoutReferences: (sentence) => closeGaps(sentence.replace(MARK, ' ').replace(QUOTE_MARKS, '')),
  };
}

function answerSentences(reply: readonly ReplyPiece[]): AnswerSentence[] {
  const { raw, numbers } = markReply(reply);
  return sentencesOf(raw, replyReader(numbers));
}

export function citing(sentences: readonly AnswerSentence[], citationNumber: number): string[] {
  return sentences
    .filter((sentence) => sentence.text && sentence.numbers.includes(citationNumber))
    .map((sentence) => sentence.text);
}

export function byOccurrence(sentences: readonly AnswerSentence[], citationNumber: number): string[] {
  return sentences.flatMap((sentence) =>
    sentence.numbers.filter((number) => number === citationNumber).map(() => sentence.text)
  );
}

/**
 * The sentences of the answer that cite source `citationNumber`, with the
 * references taken out — a matcher should not be given a number to look for.
 */
export function sentencesCiting(reply: readonly ReplyPiece[], citationNumber: number): string[] {
  return citing(answerSentences(reply), citationNumber);
}

/**
 * The sentence each chip of one source sits in, one entry per chip in reading
 * order — entry `k` answers the chip stamped `data-cite-at="k"`.
 *
 * `CitationMarks` draws the same references in the same order, so the two
 * count alike.
 */
export function sentencesByOccurrence(reply: readonly ReplyPiece[], citationNumber: number): string[] {
  return byOccurrence(answerSentences(reply), citationNumber);
}
