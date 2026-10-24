import { sentenceSpans, stripCitationMarkers } from '@/features/reading/model/sourcedPassages';

const CITATION_MARKER = /\[(\d{1,4}(?:\s*,\s*\d{1,4})*)\]/g;
const BLOCK_PREFIX = /^\s*(?:[-*+]\s+|\d{1,3}[.)]\s+|#{1,6}\s+|>\s*|\|\s*)/;
const FENCE = /^\s*(?:```|~~~)/;
const INLINE_CODE = /`[^`\n]*`/g;
const EMPHASIS = /(\*\*|__|\*|_|`)/g;

function cleanLine(line: string): string {
  return line
    .replace(BLOCK_PREFIX, '')
    .replace(/\s*\|\s*$/, '')
    .replace(EMPHASIS, '')
    .replace(/\s+/g, ' ')
    .trim();
}

/** Splits markdown text into individual sentences, ignoring fenced code blocks. */
export function splitAnswerSentences(text: string): string[] {
  const sentences: string[] = [];
  let fenced = false;
  for (const line of text.split('\n')) {
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

/** Returns all citation numbers found within a sentence. */
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

function withoutMarkers(sentence: string): string {
  return (
    stripCitationMarkers(sentence)
      .replace(/\s+/g, ' ')
      .replace(/\s+([.,;:!?])/g, '$1')
      .trim()
  );
}

/** Returns cleaned sentences from text that cite a given citation number. */
export function sentencesCiting(text: string, citationNumber: number): string[] {
  const sentences: string[] = [];
  for (const sentence of splitAnswerSentences(text)) {
    if (!citationsIn(sentence).includes(citationNumber)) continue;
    const stripped = withoutMarkers(sentence);
    if (stripped) sentences.push(stripped);
  }
  return sentences;
}

/** Returns the sentence corresponding to each occurrence of a citation chip. */
export function sentencesByOccurrence(text: string, citationNumber: number): string[] {
  const chip = new RegExp(`\\[${citationNumber}\\]`, 'g');
  const prose = text
    .split('\n')
    .map((line) => (FENCE.test(line) ? line : line.replace(INLINE_CODE, ' ')))
    .join('\n');
  const sentences: string[] = [];
  for (const sentence of splitAnswerSentences(prose)) {
    const chips = sentence.match(chip)?.length ?? 0;
    const stripped = withoutMarkers(sentence);
    for (let i = 0; i < chips; i += 1) sentences.push(stripped);
  }
  return sentences;
}
