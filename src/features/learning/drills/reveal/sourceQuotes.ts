/** A run of the source passage: plain, or the words that settle an issue. */
export interface QuotePiece {
  text: string;
  quoted: boolean;
  /** The words of the issue the learner is looking at. */
  focused: boolean;
}

/**
 * The passage cut into runs, with each quote marked where it stands in it. A quote the passage does not hold exactly
 * is left out, and quotes that overlap are one run, focused if any of them is.
 */
export function quotePieces(text: string, quotes: readonly string[], focus: string | null = null): QuotePiece[] {
  const found = quotes
    .filter((quote) => quote !== '')
    .flatMap((quote) => {
      const start = text.indexOf(quote);
      return start < 0 ? [] : [{ start, end: start + quote.length, focused: quote === focus }];
    })
    .sort((a, b) => a.start - b.start || a.end - b.end);
  const runs: { start: number; end: number; focused: boolean }[] = [];
  for (const range of found) {
    const last = runs[runs.length - 1];
    if (last && range.start <= last.end) {
      last.end = Math.max(last.end, range.end);
      last.focused ||= range.focused;
    } else {
      runs.push({ ...range });
    }
  }
  const pieces: QuotePiece[] = [];
  let at = 0;
  for (const run of runs) {
    if (run.start > at) pieces.push({ text: text.slice(at, run.start), quoted: false, focused: false });
    pieces.push({ text: text.slice(run.start, run.end), quoted: true, focused: run.focused });
    at = run.end;
  }
  if (at < text.length) pieces.push({ text: text.slice(at), quoted: false, focused: false });
  return pieces;
}
