import type { ExplorerLineRange } from '@/features/explorer/stores/explorerStore';

/** One sentence of an explanation: the line of the file it is on, which is the number the review and the key use. */
export interface Sentence {
  line: number;
  text: string;
}

/** The paragraphs of an explanation, each as its sentences. A blank line ends a paragraph and still counts as a line. */
export function paragraphsOf(text: string): Sentence[][] {
  const paragraphs: Sentence[][] = [];
  let current: Sentence[] = [];
  text.split('\n').forEach((raw, index) => {
    const sentence = raw.trim();
    if (sentence === '') {
      if (current.length > 0) paragraphs.push(current);
      current = [];
      return;
    }
    current.push({ line: index + 1, text: sentence });
  });
  if (current.length > 0) paragraphs.push(current);
  return paragraphs;
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
