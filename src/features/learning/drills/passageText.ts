import type { ExplorerLineRange } from '@/features/explorer/stores/explorerStore';

/** One sentence of an explanation: the line of the file it is on, which is the number the review and the key use. */
export interface Sentence {
  line: number;
  text: string;
}

/** The sentences of an explanation, one to a line, each with the number of its line. */
export function sentencesOf(text: string): Sentence[] {
  return text
    .split('\n')
    .map((raw, index) => ({ line: index + 1, text: raw.trim() }))
    .filter((sentence) => sentence.text !== '');
}

/**
 * The sentences picked once the sentence on `line` is chosen: a plain choice picks it alone, and choosing the one
 * sentence picked clears it; extending runs from the first picked sentence to this one, as the gutter of a code view does.
 */
export function pickSentence(existing: ExplorerLineRange | null, line: number, extend: boolean): ExplorerLineRange | null {
  if (extend && existing) {
    return { startLine: Math.min(existing.startLine, line), endLine: Math.max(existing.startLine, line) };
  }
  if (!extend && existing?.startLine === line && existing.endLine === line) return null;
  return { startLine: line, endLine: line };
}
