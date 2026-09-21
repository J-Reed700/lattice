/**
 * The sentences of an answer that cite a particular source.
 *
 * Used when a turn was never verified: with no claim verdicts to read, the only
 * record of which sentences leaned on source `n` is the `[n]` markers in the
 * answer text itself.
 */

import { sentenceSpans, stripCitationMarkers } from './sourcedPassages';

/** `[3]`, `[1][3]` and `[1, 3]`. The chips only draw `[n]`; answers write both. */
const CITATION_MARKER = /\[(\d{1,4}(?:\s*,\s*\d{1,4})*)\]/g;

/** Markdown that carries no words: list bullets, headings, quotes, table pipes. */
const BLOCK_PREFIX = /^\s*(?:[-*+]\s+|\d{1,3}[.)]\s+|#{1,6}\s+|>\s*|\|\s*)/;

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
  for (const line of text.split('\n')) {
    const cleaned = cleanLine(line);
    if (!cleaned) continue;
    for (const span of sentenceSpans(cleaned)) {
      const sentence = cleaned.slice(span.start, span.end).trim();
      if (sentence) sentences.push(sentence);
    }
  }
  return sentences;
}

/** The citation numbers a sentence carries, in the order they are written. */
export function citationsIn(sentence: string): number[] {
  const numbers: number[] = [];
  for (const match of sentence.matchAll(CITATION_MARKER)) {
    for (const part of match[1]!.split(',')) {
      const number = Number(part.trim());
      if (Number.isInteger(number) && number > 0) numbers.push(number);
    }
  }
  return numbers;
}

/**
 * The sentences of `text` that cite source `citationNumber`, with the markers
 * taken out — a matcher should not be given `[4]` to look for.
 */
export function sentencesCiting(text: string, citationNumber: number): string[] {
  const sentences: string[] = [];
  for (const sentence of splitAnswerSentences(text)) {
    if (!citationsIn(sentence).includes(citationNumber)) continue;
    const stripped = stripCitationMarkers(sentence)
      .replace(/\s+/g, ' ')
      // The marker sat before the full stop; the gap it leaves would otherwise
      // show up in the tooltip that names the sentence.
      .replace(/\s+([.,;:!?])/g, '$1')
      .trim();
    if (stripped) sentences.push(stripped);
  }
  return sentences;
}
