/**
 * How the article reader cuts saved page text into the blocks it draws.
 *
 * Offsets are into the text as saved, so the passages a reader marks (found by
 * offset in the same text) land in the blocks unchanged. Markdown syntax is
 * never removed from the text: a block's ranges only say which characters are
 * its words, and the characters left out are the syntax.
 */

export type ArticleBlockKind = 'paragraph' | 'heading' | 'list' | 'code' | 'quote' | 'table' | 'rule';

export interface TextRange {
  start: number;
  end: number;
}

export interface ArticleListItem extends TextRange {
  /** 0 is the outermost list. */
  depth: number;
  ordered: boolean;
  /** The number the item's own marker wrote, for an ordered item. */
  number?: number;
}

export interface ArticleBlock {
  start: number;
  end: number;
  /** How the block reads. Saved course text keeps its Markdown headings, lists and code. */
  kind: ArticleBlockKind;
  /** A heading's level, 1 to 6. */
  level?: number;
  /** A list's items: each item's content range (indent, marker and the space after it left out). */
  items?: ArticleListItem[];
  /** A quote's lines: each line's content (the `>` and one space after it left out). */
  lines?: TextRange[];
  /** A table's rows, header first and the delimiter row left out; each cell is trimmed. */
  rows?: TextRange[][];
  /** A table's column alignment, from its delimiter row. */
  align?: Array<'left' | 'center' | 'right' | null>;
  /** A code block's language: the first word of the opening fence's info string. */
  language?: string;
}

const HEADING = /^ {0,3}(#{1,6})[ \t]+/;
/** A list item's line: its indentation, its marker, and the space after it. */
const LIST_LINE = /^([ \t]*)([-*+]|\d{1,9}[.)])[ \t]/;
const FENCE = /^\s*(`{3,}|~{3,})/;
const RULE = /^ {0,3}([-*_])(?:[ \t]*\1){2,}[ \t]*$/;
const SETEXT = /^ {0,3}(=+|-+)[ \t]*$/;
const QUOTE = /^ {0,3}>[ \t]?/;
const DELIMITER_CELL = /^:?-+:?$/;

interface Line {
  /** Start of the line, indentation included. */
  s: number;
  /** End of the line's content: the line break and trailing whitespace left out. */
  e: number;
}

function indentWidth(indent: string): number {
  let width = 0;
  for (const char of indent) width += char === '\t' ? 4 : 1;
  return width;
}

/**
 * Blocks, as the page cache separates them: a blank line apart. A fenced code
 * block stays whole across its blank lines, and its span is exactly its lines
 * (the fence lines and the last line break, LF or CRLF, are left out).
 *
 * A Markdown heading is its own block with its markers (and any closing `#`s)
 * left out of the span, so a passage that starts on it still marks the
 * heading's words. As in CommonMark, a heading may start on any line of a
 * block: the lines before it become their own block and the text after the
 * heading line is another. A heading made only of markers is dropped. A
 * paragraph followed by a `===` or `---` underline is a heading too.
 *
 * Lists, quotes and GFM tables read as their own blocks, and a lead-in line
 * before a list or table is a paragraph of its own. A block of one line of
 * `---`, `***` or `___` is a rule.
 */
export function articleBlocks(text: string): ArticleBlock[] {
  const blocks: ArticleBlock[] = [];

  const lineStartOf = (line: Line): number => {
    let at = line.s;
    while (at < line.e && (text[at] === ' ' || text[at] === '\t')) at += 1;
    return at;
  };

  /** Where each cell of a table row's text lies; pipes inside code or escaped stay in their cell. */
  const splitRow = (line: Line): TextRange[] => {
    let from = lineStartOf(line);
    let to = line.e;
    if (text[from] === '|') from += 1;
    if (to > from && text[to - 1] === '|') {
      let slashes = 0;
      while (to - 2 - slashes >= from && text[to - 2 - slashes] === '\\') slashes += 1;
      if (slashes % 2 === 0) to -= 1;
    }
    const cells: TextRange[] = [];
    const push = (a: number, b: number) => {
      let start = a;
      let end = b;
      while (start < end && /\s/.test(text[start]!)) start += 1;
      while (end > start && /\s/.test(text[end - 1]!)) end -= 1;
      cells.push(start < end ? { start, end } : { start: a, end: a });
    };
    let cell = from;
    let at = from;
    while (at < to) {
      const char = text[at];
      if (char === '\\') {
        at += 2;
      } else if (char === '`') {
        let ticks = 0;
        while (at + ticks < to && text[at + ticks] === '`') ticks += 1;
        let close = -1;
        let scan = at + ticks;
        while (scan < to) {
          if (text[scan] !== '`') {
            scan += 1;
            continue;
          }
          let run = 0;
          while (scan + run < to && text[scan + run] === '`') run += 1;
          if (run === ticks) {
            close = scan + run;
            break;
          }
          scan += run;
        }
        at = close >= 0 ? close : at + ticks;
      } else if (char === '|') {
        push(cell, at);
        at += 1;
        cell = at;
      } else {
        at += 1;
      }
    }
    push(cell, Math.max(to, cell));
    return cells;
  };

  /** The alignment a delimiter row writes, or null when the line is not one. */
  const delimiterRow = (line: Line): Array<'left' | 'center' | 'right' | null> | null => {
    if (!text.slice(line.s, line.e).includes('|')) return null;
    const cells = splitRow(line);
    const align: Array<'left' | 'center' | 'right' | null> = [];
    for (const cell of cells) {
      const word = text.slice(cell.start, cell.end);
      if (!DELIMITER_CELL.test(word)) return null;
      const left = word.startsWith(':');
      const right = word.endsWith(':');
      align.push(left && right ? 'center' : right ? 'right' : left ? 'left' : null);
    }
    return align;
  };

  const pushChunk = (from: number, to: number) => {
    const lines: Line[] = [];
    let at = from;
    while (at < to) {
      const newline = text.indexOf('\n', at);
      const lineEnd = newline < 0 || newline > to ? to : newline;
      let end = lineEnd;
      while (end > at && /\s/.test(text[end - 1]!)) end -= 1;
      if (end > at) lines.push({ s: at, e: end });
      at = lineEnd + 1;
    }

    let paragraph: Line[] = [];
    const flushParagraph = () => {
      if (paragraph.length === 0) return;
      blocks.push({
        start: lineStartOf(paragraph[0]!),
        end: paragraph[paragraph.length - 1]!.e,
        kind: 'paragraph',
      });
      paragraph = [];
    };

    let index = 0;
    while (index < lines.length) {
      const line = lines[index]!;
      const lineText = text.slice(line.s, line.e);

      const heading = HEADING.exec(lineText);
      if (heading) {
        flushParagraph();
        const wordsStart = line.s + heading[0].length;
        let wordsEnd = line.e;
        // "## Title ##" closes with markers too.
        while (wordsEnd > line.s && /[#\s]/.test(text[wordsEnd - 1]!)) wordsEnd -= 1;
        if (wordsEnd > wordsStart) {
          blocks.push({ start: wordsStart, end: wordsEnd, kind: 'heading', level: heading[1]!.length });
        }
        index += 1;
        continue;
      }

      if (paragraph.length > 0) {
        const underline = SETEXT.exec(lineText);
        if (underline && underline[1]!.length >= 2) {
          blocks.push({
            start: lineStartOf(paragraph[0]!),
            end: paragraph[paragraph.length - 1]!.e,
            kind: 'heading',
            level: underline[1]![0] === '=' ? 1 : 2,
          });
          paragraph = [];
          index += 1;
          continue;
        }
      } else if (RULE.test(lineText)) {
        blocks.push({ start: lineStartOf(line), end: line.e, kind: 'rule' });
        index += 1;
        continue;
      }

      // A table: a line with a pipe, then a delimiter row for as many columns.
      const next = lines[index + 1];
      if (next && lineText.includes('|')) {
        const align = delimiterRow(next);
        const header = align ? splitRow(line) : null;
        if (align && header?.length === align.length) {
          flushParagraph();
          const rows: TextRange[][] = [header];
          let last = next;
          index += 2;
          while (index < lines.length) {
            const row = lines[index]!;
            const rowText = text.slice(row.s, row.e);
            if (!rowText.includes('|') || HEADING.test(rowText)) break;
            rows.push(splitRow(row));
            last = row;
            index += 1;
          }
          blocks.push({ start: lineStartOf(line), end: last.e, kind: 'table', rows, align });
          continue;
        }
      }

      const listMatch = LIST_LINE.exec(lineText);
      // As in CommonMark, an ordered item interrupts a paragraph only as number 1:
      // hard-wrapped text often breaks a line before "2004. The study...".
      const continuesParagraph =
        paragraph.length > 0 && listMatch !== null && /\d/.test(listMatch[2]![0]!) && Number.parseInt(listMatch[2]!, 10) !== 1;
      if (listMatch && !continuesParagraph) {
        flushParagraph();
        const items: ArticleListItem[] = [];
        const indents: number[] = [];
        const first = lineStartOf(line);
        let last = line;
        while (index < lines.length) {
          const current = lines[index]!;
          const currentText = text.slice(current.s, current.e);
          if (HEADING.test(currentText)) break;
          const item = LIST_LINE.exec(currentText);
          if (item) {
            const width = indentWidth(item[1]!);
            while (indents.length > 0 && indents[indents.length - 1]! > width) indents.pop();
            if (indents.length === 0 || indents[indents.length - 1]! < width) indents.push(width);
            let contentStart = current.s + item[0].length;
            while (contentStart < current.e && (text[contentStart] === ' ' || text[contentStart] === '\t')) {
              contentStart += 1;
            }
            const ordered = /\d/.test(item[2]![0]!);
            items.push({
              start: contentStart,
              end: current.e,
              depth: indents.length - 1,
              ordered,
              ...(ordered ? { number: Number.parseInt(item[2]!, 10) } : {}),
            });
          } else {
            // A line that is no item carries on the item before it.
            items[items.length - 1]!.end = current.e;
          }
          last = current;
          index += 1;
        }
        blocks.push({ start: first, end: last.e, kind: 'list', items });
        continue;
      }

      if (paragraph.length === 0 && QUOTE.test(lineText)) {
        const quoteLines: TextRange[] = [];
        const first = lineStartOf(line);
        let last = line;
        while (index < lines.length) {
          const current = lines[index]!;
          const currentText = text.slice(current.s, current.e);
          if (HEADING.test(currentText)) break;
          const marker = QUOTE.exec(currentText);
          quoteLines.push({ start: marker ? current.s + marker[0].length : lineStartOf(current), end: current.e });
          last = current;
          index += 1;
        }
        blocks.push({ start: first, end: last.e, kind: 'quote', lines: quoteLines });
        continue;
      }

      paragraph.push(line);
      index += 1;
    }
    flushParagraph();
  };

  const pushCode = (openLineEnd: number, closeLineStart: number, language: string | undefined) => {
    const start = Math.min(openLineEnd + 1, closeLineStart);
    let end = closeLineStart;
    if (end > start && text[end - 1] === '\n') end -= 1;
    if (end > start && text[end - 1] === '\r') end -= 1;
    if (end > start && text.slice(start, end).trim()) {
      blocks.push({ start, end, kind: 'code', ...(language ? { language } : {}) });
    }
  };

  let blockStart = 0;
  let fence: string | null = null;
  let fenceLineEnd = 0;
  let fenceLanguage: string | undefined;
  let lineStart = 0;
  for (;;) {
    const newline = text.indexOf('\n', lineStart);
    const lineEnd = newline < 0 ? text.length : newline;
    const line = text.slice(lineStart, lineEnd);
    const marker = FENCE.exec(line)?.[1];
    if (fence) {
      if (marker?.[0] === fence[0] && marker.length >= fence.length) {
        pushCode(fenceLineEnd, lineStart, fenceLanguage);
        fence = null;
        blockStart = lineEnd + 1;
      }
    } else if (marker) {
      pushChunk(blockStart, lineStart);
      fence = marker;
      fenceLineEnd = lineEnd;
      const info = line.slice(line.indexOf(marker) + marker.length).trim().split(/\s+/)[0];
      fenceLanguage = info || undefined;
    } else if (!line.trim()) {
      pushChunk(blockStart, lineStart);
      blockStart = lineEnd + 1;
    }
    if (newline < 0) break;
    lineStart = newline + 1;
  }
  if (fence) pushCode(fenceLineEnd, text.length, fenceLanguage);
  else pushChunk(blockStart, text.length);
  return blocks;
}
