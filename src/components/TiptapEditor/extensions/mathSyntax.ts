import type MarkdownIt from 'markdown-it';

/**
 * TeX math in markdown, for the read-only viewer.
 *
 * Inline: `$…$`, `\(…\)`, and `$$…$$` inside a paragraph. Display: `$$…$$`
 * or `\[…\]` starting a line of its own, on one line or across several with
 * no blank line between. The dollar rules are Pandoc's, so prose keeps its
 * prices: an opening `$` has a non-space after it, the closing `$` has a
 * non-space before it and no digit after it (`$5 and $10` stays text), `\$`
 * is a literal dollar, and a `$` straight after another never opens one.
 * A formula never holds a backtick, so a code span keeps its dollars, and an
 * unclosed opener (an answer still arriving) stays text until it closes.
 *
 * The Rust checks read answers by the same rules:
 * crates/lattice-core/src/shared/markdown_math.rs.
 * Design: docs/design/2026-10-19-math-rendering.md.
 */

export type MathDelimiter = '$' | '$$' | '\\(' | '\\[';

const CLOSER: Record<MathDelimiter, string> = { $: '$', $$: '$$', '\\(': '\\)', '\\[': '\\]' };
const DISPLAY_OPENERS = ['$$', '\\['] as const;
const SPACE = /\s/;
const DIGIT = /[0-9]/;

export interface FoundMath {
  latex: string;
  delimiter: MathDelimiter;
  /** Just past the closing delimiter. */
  end: number;
}

type InlineRule = Parameters<MarkdownIt['inline']['ruler']['before']>[2];
type BlockRule = Parameters<MarkdownIt['block']['ruler']['before']>[2];

/** Where `closer` first stands in `src[from, max)`, stepping over `\x` escapes; -1 if nowhere. */
function closerAt(src: string, from: number, max: number, closer: string): number {
  for (let i = from; i + closer.length <= max; i += 1) {
    if (src.startsWith(closer, i)) return i;
    if (src[i] === '\\') i += 1;
  }
  return -1;
}

/** The inline formula opening at `pos` in `src[0, max)`, or null when none does. */
export function inlineMathAt(src: string, pos: number, max: number): FoundMath | null {
  let delimiter: MathDelimiter;
  if (src.startsWith('\\(', pos)) delimiter = '\\(';
  else if (src.startsWith('$$', pos)) delimiter = '$$';
  else if (src[pos] === '$' && src[pos - 1] !== '$') delimiter = '$';
  else return null;

  const from = pos + delimiter.length;
  const closer = CLOSER[delimiter];
  const close = closerAt(src, from, max, closer);
  if (close === -1) return null;
  const latex = src.slice(from, close);
  if (!latex.trim() || latex.includes('`')) return null;
  if (delimiter === '$') {
    const after = close + 1 < max ? src[close + 1] ?? '' : '';
    if (SPACE.test(src[from] ?? '') || SPACE.test(src[close - 1] ?? '') || DIGIT.test(after)) return null;
  }
  return { latex, delimiter, end: close + closer.length };
}

const inlineMath: InlineRule = (state, silent) => {
  const found = inlineMathAt(state.src, state.pos, state.posMax);
  if (!found) return false;
  if (!silent) {
    const token = state.push('math_inline', 'span', 0);
    token.content = found.latex;
    token.markup = found.delimiter;
  }
  state.pos = found.end;
  return true;
};

const displayMath: BlockRule = (state, startLine, endLine, silent) => {
  if (state.sCount[startLine] - state.blkIndent >= 4) return false;
  const start = state.bMarks[startLine] + state.tShift[startLine];
  const opener = DISPLAY_OPENERS.find((candidate) => state.src.startsWith(candidate, start));
  if (!opener) return false;
  const closer = CLOSER[opener];

  const lines: string[] = [];
  let line = startLine;
  let from = start + opener.length;
  for (;;) {
    const max = state.eMarks[line];
    const close = closerAt(state.src, from, max, closer);
    if (close !== -1) {
      // Text after the closer makes it a formula inside a paragraph.
      if (state.src.slice(close + closer.length, max).trim()) return false;
      lines.push(state.src.slice(from, close));
      break;
    }
    lines.push(state.src.slice(from, max));
    line += 1;
    // A blank line, the end of its container or of the text: unclosed, so text.
    if (line >= endLine || state.isEmpty(line) || state.sCount[line] < state.blkIndent) return false;
    from = state.bMarks[line] + state.tShift[line];
  }

  const latex = lines.join('\n');
  if (!latex.trim()) return false;
  if (silent) return true;
  const token = state.push('math_block', 'div', 0);
  token.block = true;
  token.content = latex;
  token.markup = opener;
  token.map = [startLine, line + 1];
  state.line = line + 1;
  return true;
};

const prepared = new WeakSet<MarkdownIt>();

/**
 * Teaches markdown-it the formulas above, rendered as the elements the math
 * nodes parse. tiptap-markdown runs every node's setup on each parse with the
 * same instance, so this registers once per instance.
 */
export function addMathSyntax(md: MarkdownIt): void {
  if (prepared.has(md)) return;
  prepared.add(md);
  // Before `escape`, which would read `\(` and `\[` as literal brackets.
  md.inline.ruler.before('escape', 'math_inline', inlineMath);
  md.block.ruler.before('fence', 'math_block', displayMath, {
    alt: ['paragraph', 'reference', 'blockquote', 'list'],
  });
  const element = (tag: string, type: string, latex: string, delimiter: string) =>
    `<${tag} data-type="${type}" data-latex="${md.utils.escapeHtml(latex)}" data-delimiter="${md.utils.escapeHtml(delimiter)}"></${tag}>`;
  md.renderer.rules.math_inline = (tokens, idx) => {
    const token = tokens[idx];
    return element('span', 'inline-math', token.content, token.markup);
  };
  md.renderer.rules.math_block = (tokens, idx) => {
    const token = tokens[idx];
    return `${element('div', 'block-math', token.content, token.markup)}\n`;
  };
}

/** The markdown a formula was written as, from its LaTeX and opening delimiter. */
export function mathSource(latex: string, delimiter: string): string {
  const closer = CLOSER[delimiter as MathDelimiter] ?? delimiter;
  return `${delimiter}${latex}${closer}`;
}
