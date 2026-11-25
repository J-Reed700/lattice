import { articleBlocks, type ArticleBlock, type TextRange } from '@/features/reading/model/articleBlocks';

/**
 * The inline Markdown of saved page text, as runs of the characters a reader
 * sees.
 *
 * Offsets are into the full text and never move: a run says which source range
 * it displays, and the syntax around it (delimiters, brackets, link
 * destinations, escape backslashes) belongs to no run. A CommonMark-lite
 * subset is read; anything else is literal text, so nothing is ever dropped.
 */

export interface InlineRun {
  /** The source range this run displays, in the full text. */
  start: number;
  end: number;
  /** What it shows. Equals `text.slice(start, end)` unless the run is `atomic`. */
  text: string;
  /**
   * Shows something other than its source (an escape or an entity), so it
   * displays whole, is never split, and never merges with its neighbours.
   */
  atomic?: true;
  strong?: true;
  em?: true;
  code?: true;
  strike?: true;
  image?: true;
  /** Only for http(s): destinations: the only ones the reader can open. */
  href?: string;
}

type Flags = Pick<InlineRun, 'strong' | 'em' | 'strike'>;

interface Delimiter {
  ch: '*' | '_' | '~';
  /** The part of the source run not yet used by a match: [lo, hi). */
  lo: number;
  hi: number;
  canOpen: boolean;
  canClose: boolean;
}

interface Item {
  runs: InlineRun[];
  flags: Flags;
  delimiter?: Delimiter;
}

interface Context {
  text: string;
  /** Where each `[` that closes, closes. */
  closeOf: Map<number, number>;
  /** Positions of backtick runs, by run length. */
  ticks: Map<number, number[]>;
  /** What is left of the budget for scanning link destinations. */
  budget: number;
}

const PUNCTUATION = /[!-/:-@[-`{-~]/;
/** Shared by every item that no emphasis encloses. Frozen: a write that skips the copy-on-write would style every such item. */
const NO_FLAGS: Flags = Object.freeze({});
const ALNUM = /[\p{L}\p{N}]/u;
const ENTITY = /^&(?:(amp|lt|gt|quot|apos|nbsp)|#(\d{1,7})|#[xX]([0-9a-fA-F]{1,6}));/;
const NAMED_ENTITIES: Record<string, string> = {
  amp: '&',
  lt: '<',
  gt: '>',
  quot: '"',
  apos: "'",
  nbsp: ' ',
};
const AUTOLINK = /^<((?:https?:\/\/|mailto:)[^\s<>]*)>/i;
const SAFE_HREF = /^https?:/i;
/** Labels nest at most this deep; past it a `[` is just a character. */
const MAX_LABEL_DEPTH = 6;
/** Characters of link destination one call may scan, so a hostile paragraph cannot go quadratic. */
export const SCAN_BUDGET = 300_000;
const MAX_AUTOLINK = 2048;

function decodeEntityAt(text: string, at: number, end: number): { text: string; length: number } | null {
  const match = ENTITY.exec(text.slice(at, Math.min(end, at + 12)));
  if (!match) return null;
  if (match[1]) return { text: NAMED_ENTITIES[match[1]]!, length: match[0].length };
  const code = match[2] ? Number.parseInt(match[2], 10) : Number.parseInt(match[3]!, 16);
  if (code <= 0 || code > 0x10ffff || (code >= 0xd800 && code <= 0xdfff)) return null;
  return { text: String.fromCodePoint(code), length: match[0].length };
}

/** The first position in `positions` (ascending) that is at least `from`. */
function firstAtLeast(positions: readonly number[] | undefined, from: number): number {
  if (!positions) return -1;
  let low = 0;
  let high = positions.length;
  while (low < high) {
    const mid = (low + high) >> 1;
    if (positions[mid]! < from) low = mid + 1;
    else high = mid;
  }
  return low < positions.length ? positions[low]! : -1;
}

/** Where the next backtick run of exactly `length` starts, at or after `from`; -1 when there is none. */
function codeCloser(context: Context, from: number, length: number): number {
  return firstAtLeast(context.ticks.get(length), from);
}

function countOf(text: string, at: number, end: number, char: string): number {
  let count = 0;
  while (at + count < end && text[at + count] === char) count += 1;
  return count;
}

function analyse(text: string, start: number, end: number): Context {
  const ticks = new Map<number, number[]>();
  for (let at = start; at < end; ) {
    if (text[at] !== '`') {
      at += 1;
      continue;
    }
    const length = countOf(text, at, end, '`');
    const positions = ticks.get(length);
    if (positions) positions.push(at);
    else ticks.set(length, [at]);
    at += length;
  }

  const context: Context = { text, closeOf: new Map(), ticks, budget: SCAN_BUDGET };
  const open: number[] = [];
  for (let at = start; at < end; ) {
    const char = text[at];
    if (char === '\\' && at + 1 < end && PUNCTUATION.test(text[at + 1]!)) {
      at += 2;
    } else if (char === '`') {
      const length = countOf(text, at, end, '`');
      const closer = codeCloser(context, at + length, length);
      at = closer >= 0 ? closer + length : at + length;
    } else {
      if (char === '[') open.push(at);
      else if (char === ']') {
        const opener = open.pop();
        if (opener !== undefined) context.closeOf.set(opener, at);
      }
      at += 1;
    }
  }
  return context;
}

interface LinkParts {
  /** The text after the closing parenthesis. */
  after: number;
  href: string | null;
}

/** Read `(dest "title")` starting at the `(` at `open`. Null when it is not one. */
function readDestination(context: Context, open: number, end: number): LinkParts | null {
  const { text } = context;
  if (context.budget <= 0) return null;
  const started = open;
  const skipSpace = (at: number) => {
    while (at < end && /[ \t\n]/.test(text[at]!)) at += 1;
    return at;
  };
  let at = skipSpace(open + 1);
  try {
    let destStart = at;
    let destEnd = at;
    if (text[at] === '<') {
      destStart = at + 1;
      at = destStart;
      while (at < end && text[at] !== '>') {
        if (text[at] === '\n' || text[at] === '<') return null;
        at += text[at] === '\\' ? 2 : 1;
      }
      if (at >= end) return null;
      destEnd = at;
      at += 1;
    } else {
      let depth = 0;
      while (at < end) {
        const char = text[at]!;
        if (/\s/.test(char) || char < ' ') break;
        if (char === '\\' && at + 1 < end && PUNCTUATION.test(text[at + 1]!)) {
          at += 2;
          continue;
        }
        if (char === '(') depth += 1;
        else if (char === ')') {
          if (depth === 0) break;
          depth -= 1;
        }
        at += 1;
      }
      if (depth !== 0) return null;
      destEnd = at;
    }
    at = skipSpace(at);
    const quote = text[at];
    if (quote === '"' || quote === "'" || quote === '(') {
      const closing = quote === '(' ? ')' : quote;
      at += 1;
      while (at < end && text[at] !== closing) at += text[at] === '\\' ? 2 : 1;
      if (at >= end) return null;
      at = skipSpace(at + 1);
    }
    if (text[at] !== ')') return null;

    const destination = text
      .slice(destStart, destEnd)
      .replace(/\\([!-/:-@[-`{-~])/g, '$1')
      .replace(/&(?:amp|lt|gt|quot|apos|#\d{1,7}|#[xX][0-9a-fA-F]{1,6});/g, (entity) => decodeEntityAt(entity, 0, entity.length)?.text ?? entity);
    return { after: at + 1, href: SAFE_HREF.test(destination) ? destination : null };
  } finally {
    context.budget -= at - started;
  }
}

function isSpace(char: string | undefined): boolean {
  return char === undefined || /\s/.test(char);
}

function sameStyle(a: InlineRun, b: InlineRun): boolean {
  return (
    a.strong === b.strong &&
    a.em === b.em &&
    a.code === b.code &&
    a.strike === b.strike &&
    a.image === b.image &&
    a.href === b.href
  );
}

/** A run of source text as written. Its `text` is filled in by `inlineRuns`, once neighbours have merged. */
function literal(start: number, end: number): InlineRun {
  return { start, end, text: '' };
}

function parse(context: Context, start: number, end: number, depth: number): InlineRun[] {
  const { text } = context;
  const items: Item[] = [];
  const openers: Record<'*' | '_' | '~', number[]> = { '*': [], _: [], '~': [] };
  /** Matched pairs, by the items they enclose: [from, to). Applied together at the end, so deep nesting stays linear. */
  const pairs: Array<{ from: number; to: number; kind: 'strong' | 'em' | 'strike' }> = [];
  let plain = start;

  const flush = (to: number) => {
    if (to > plain) items.push({ runs: [literal(plain, to)], flags: NO_FLAGS });
  };
  const push = (run: InlineRun) => {
    items.push({ runs: [run], flags: NO_FLAGS });
  };

  const settle = (index: number) => {
    const closer = items[index]!.delimiter!;
    if (closer.canClose) {
      const stack = openers[closer.ch];
      while (closer.hi > closer.lo && stack.length > 0) {
        const openerIndex = stack[stack.length - 1]!;
        const opener = items[openerIndex]!.delimiter!;
        const strong = closer.ch === '~' || (opener.hi - opener.lo >= 2 && closer.hi - closer.lo >= 2);
        const use = strong ? 2 : 1;
        opener.hi -= use;
        closer.lo += use;
        pairs.push({ from: openerIndex + 1, to: index, kind: closer.ch === '~' ? 'strike' : strong ? 'strong' : 'em' });
        // Whatever opened inside this pair is now literal.
        for (const other of ['*', '_', '~'] as const) {
          if (other === closer.ch) continue;
          const otherStack = openers[other];
          while (otherStack.length > 0 && otherStack[otherStack.length - 1]! > openerIndex) otherStack.pop();
        }
        if (opener.hi === opener.lo) stack.pop();
      }
    }
    if (closer.hi > closer.lo && closer.canOpen) openers[closer.ch].push(index);
  };

  let at = start;
  while (at < end) {
    const char = text[at]!;

    if (char === '\\' && at + 1 < end && PUNCTUATION.test(text[at + 1]!)) {
      flush(at);
      push({ start: at, end: at + 2, text: text[at + 1]!, atomic: true });
      at += 2;
      plain = at;
      continue;
    }

    if (char === '&') {
      const entity = decodeEntityAt(text, at, end);
      if (entity) {
        flush(at);
        push({ start: at, end: at + entity.length, text: entity.text, atomic: true });
        at += entity.length;
        plain = at;
        continue;
      }
    }

    if (char === '`') {
      const length = countOf(text, at, end, '`');
      const closer = codeCloser(context, at + length, length);
      if (closer >= 0 && closer + length <= end) {
        let from = at + length;
        let to = closer;
        if (to - from >= 2 && text[from] === ' ' && text[to - 1] === ' ' && /[^ ]/.test(text.slice(from, to))) {
          from += 1;
          to -= 1;
        }
        flush(at);
        if (to > from) push({ ...literal(from, to), code: true });
        at = closer + length;
        plain = at;
      } else {
        at += length;
      }
      continue;
    }

    if (char === '<') {
      const autolink = AUTOLINK.exec(text.slice(at, Math.min(end, at + MAX_AUTOLINK)));
      if (autolink) {
        flush(at);
        const address = autolink[1]!;
        push({
          start: at + 1,
          end: at + 1 + address.length,
          text: address,
          ...(SAFE_HREF.test(address) ? { href: address } : {}),
        });
        at += autolink[0].length;
        plain = at;
        continue;
      }
    }

    if ((char === '[' || (char === '!' && text[at + 1] === '[')) && depth < MAX_LABEL_DEPTH) {
      const image = char === '!';
      const open = image ? at + 1 : at;
      const close = context.closeOf.get(open);
      if (close !== undefined && close < end && text[close + 1] === '(') {
        const link = readDestination(context, close + 1, end);
        if (link) {
          flush(at);
          if (image) {
            if (close > open + 1) {
              push({ ...literal(open + 1, close), image: true });
            }
          } else {
            const href = link.href;
            const label = parse(context, open + 1, close, depth + 1);
            items.push({
              runs: href ? label.map((run) => (run.href === undefined ? { ...run, href } : run)) : label,
              flags: NO_FLAGS,
            });
          }
          at = link.after;
          plain = at;
          continue;
        }
      }
    }

    if (char === '*' || char === '_' || char === '~') {
      const length = countOf(text, at, end, char);
      if (char === '~' && length !== 2) {
        at += length;
        continue;
      }
      const before = at > start ? text[at - 1] : undefined;
      const after = at + length < end ? text[at + length] : undefined;
      let canOpen = !isSpace(after);
      let canClose = !isSpace(before);
      if (char === '_') {
        canOpen &&= !(before !== undefined && ALNUM.test(before));
        canClose &&= !(after !== undefined && ALNUM.test(after));
      }
      flush(at);
      items.push({ runs: [], flags: NO_FLAGS, delimiter: { ch: char, lo: at, hi: at + length, canOpen, canClose } });
      settle(items.length - 1);
      at += length;
      plain = at;
      continue;
    }

    at += 1;
  }
  flush(end);

  const depthOf = { strong: new Int32Array(items.length + 1), em: new Int32Array(items.length + 1), strike: new Int32Array(items.length + 1) };
  for (const pair of pairs) {
    depthOf[pair.kind][pair.from] += 1;
    depthOf[pair.kind][pair.to] -= 1;
  }
  const running = { strong: 0, em: 0, strike: 0 };
  items.forEach((item, position) => {
    for (const kind of ['strong', 'em', 'strike'] as const) {
      running[kind] += depthOf[kind][position]!;
      if (running[kind] > 0) {
        if (item.flags === NO_FLAGS) item.flags = {};
        item.flags[kind] = true;
      }
    }
  });

  const runs: InlineRun[] = [];
  for (const item of items) {
    const delimiter = item.delimiter;
    const own: InlineRun[] = delimiter
      ? delimiter.hi > delimiter.lo
        ? [literal(delimiter.lo, delimiter.hi)]
        : []
      : item.runs;
    const { strong, em, strike } = item.flags;
    for (const run of own) runs.push(strong || em || strike ? { ...run, ...mergedFlags(run, item.flags) } : run);
  }
  return runs;
}

/** A run keeps the styling it was given and gains its item's. */
function mergedFlags(run: InlineRun, flags: Flags): Flags {
  return {
    ...(run.strong || flags.strong ? { strong: true as const } : {}),
    ...(run.em || flags.em ? { em: true as const } : {}),
    ...(run.strike || flags.strike ? { strike: true as const } : {}),
  };
}

/**
 * The runs of `text[start, end)`, in order and without overlap. Adjacent
 * characters with the same styling are one run; an escape or an entity is a run
 * of its own, because what it shows is not what it is written as.
 */
export function inlineRuns(text: string, start: number, end: number): InlineRun[] {
  const from = Math.max(0, start);
  const to = Math.min(text.length, end);
  if (to <= from) return [];
  try {
    const context = analyse(text, from, to);
    const merged: InlineRun[] = [];
    for (const run of parse(context, from, to, 0)) {
      if (run.end <= run.start) continue;
      const previous = merged[merged.length - 1];
      if (previous?.end === run.start && !previous.atomic && !run.atomic && sameStyle(previous, run)) {
        previous.end = run.end;
      } else {
        merged.push(run);
      }
    }
    // Merged runs only grew their range; what they show is read from the source once.
    for (const run of merged) if (!run.atomic) run.text = text.slice(run.start, run.end);
    return merged;
  } catch {
    return [{ start: from, end: to, text: text.slice(from, to) }];
  }
}

/** What a run shows of `[start, end)`: an escape or entity shows wholly when it starts inside. */
function clippedText(text: string, run: InlineRun, start: number, end: number): string {
  if (run.atomic === true) {
    return run.start >= start && run.start < end ? run.text : '';
  }
  const from = Math.max(run.start, start);
  const to = Math.min(run.end, end);
  return to > from ? text.slice(from, to) : '';
}

function inlineVisible(text: string, range: TextRange, start: number, end: number): string {
  if (range.end <= start || range.start >= end) return '';
  let shown = '';
  for (const run of inlineRuns(text, range.start, range.end)) shown += clippedText(text, run, start, end);
  return shown.trim();
}

function blockVisible(text: string, block: ArticleBlock, start: number, end: number): string {
  if (block.end <= start || block.start >= end) return '';
  const pieces = (ranges: TextRange[]) =>
    ranges.map((range) => inlineVisible(text, range, start, end)).filter((piece) => piece.length > 0);
  switch (block.kind) {
    case 'rule':
      return '';
    case 'code':
      return text.slice(Math.max(block.start, start), Math.min(block.end, end)).trim();
    case 'list':
      return pieces(block.items ?? []).join('\n');
    case 'quote':
      return pieces(block.lines ?? []).join('\n');
    case 'table':
      return (block.rows ?? [])
        .map((row) => pieces(row).join(' · '))
        .filter((row) => row.length > 0)
        .join('\n');
    default:
      return inlineVisible(text, block, start, end);
  }
}

/**
 * The words a reader sees for `[start, end)` of a Markdown text: block markers
 * and inline syntax left out. Blocks are joined by a blank line, list items and
 * quote lines by a line break, table cells by " · ".
 */
export function visibleText(text: string, start = 0, end: number = text.length): string {
  return articleBlocks(text)
    .map((block) => blockVisible(text, block, start, end))
    .filter((piece) => piece.length > 0)
    .join('\n\n');
}

/**
 * The opening blocks of an article: whole blocks until their visible text
 * reaches `maxChars` (at least one block), and whether any were left out.
 */
export function blocksWithin(
  text: string,
  blocks: readonly ArticleBlock[],
  maxChars: number,
): { blocks: ArticleBlock[]; truncated: boolean } {
  const kept: ArticleBlock[] = [];
  let seen = 0;
  for (const block of blocks) {
    if (kept.length > 0 && seen >= maxChars) break;
    kept.push(block);
    seen += blockVisible(text, block, 0, text.length).length;
  }
  return { blocks: kept, truncated: kept.length < blocks.length };
}
