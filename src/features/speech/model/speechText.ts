/**
 * What read-aloud says: the prose of a markdown answer or lesson, without its
 * code, citation markers, links' addresses, math or markdown symbols, cut
 * into sentence-sized pieces. The first piece is one sentence so speech
 * starts as soon as that sentence is ready.
 */

const FENCE = /^\s*(```|~~~)/;
const DISPLAY_MATH = /\$\$[\s\S]*?\$\$/g;
/** `$x^2$`, but not the prices in "$5 and $10". */
const INLINE_MATH = /(^|[^\\$\w])\$(?![\s\d])[^$\n]*?[^\s$]\$(?!\d)/g;
const HTML_TAG = /<\/?[a-zA-Z][^>]*>/g;
const IMAGE = /!\[[^\]]*\]\([^)]*\)/g;
const LINK = /\[([^\]]+)\]\((?:[^()\s]+|\([^)]*\))*(?:\s+"[^"]*")?\)/g;
const AUTOLINK = /<(?:https?|mailto):[^>\s]+>/g;
const BARE_URL = /\b(?:https?:\/\/|www\.)[^\s<>()]+[^\s<>().,;:!?'")\]]/g;
const CITATION = /\s?\[\^?\d{1,5}(?:\s*[,–-]\s*\d{1,5})*\]/g;
const FOOTNOTE_DEFINITION = /^\s*\[\^[^\]]+\]:.*$/gm;
const INLINE_CODE = /`([^`\n]+)`/g;
const CODE_LIKE = /[{}();=<>[\]]|=>|::/;
const TIME = /\b(\d{1,2}):(\d{2})\b/g;

const ABBREVIATIONS = new Set(['e.g', 'i.e', 'etc', 'vs', 'cf', 'al', 'dr', 'mr', 'mrs', 'ms', 'prof', 'fig', 'approx']);

/** Ends a line that reads as its own sentence with a full stop, so speech pauses there. */
function closeSentence(line: string): string {
  return /[.!?…:;]$/.test(line) ? line : `${line}.`;
}

function tableRow(line: string): string | null {
  const cells = line.replace(/^\s*\|/, '').replace(/\|\s*$/, '').split('|').map((cell) => cell.trim());
  if (cells.every((cell) => /^:?-{2,}:?$/.test(cell) || cell === '')) return null;
  return closeSentence(cells.filter(Boolean).join(', '));
}

/** `text` without its fenced code blocks; an unclosed fence runs to the end. */
function withoutCode(text: string): string {
  let fence: string | null = null;
  return text
    .split('\n')
    .filter((line) => {
      const marker = FENCE.exec(line)?.[1];
      if (fence === null && marker) {
        fence = marker;
        return false;
      }
      if (fence !== null) {
        if (marker === fence) fence = null;
        return false;
      }
      return true;
    })
    .join('\n');
}

/** The prose of `markdown`, as plain sentences separated by blank lines. */
export function speakableText(markdown: string): string {
  const text = withoutCode(markdown.replace(/\r\n?/g, '\n'))
    .replace(DISPLAY_MATH, ' ')
    .replace(INLINE_MATH, '$1')
    .replace(FOOTNOTE_DEFINITION, '')
    .replace(IMAGE, '')
    .replace(LINK, '$1')
    .replace(AUTOLINK, '')
    .replace(BARE_URL, '')
    .replace(HTML_TAG, '')
    .replace(INLINE_CODE, (_match, code: string) => (CODE_LIKE.test(code) || code.length > 40 ? '' : code))
    .replace(CITATION, '')
    .replace(TIME, '$1 $2');

  const paragraphs: string[] = [];
  let current: string[] = [];
  const flush = () => {
    if (current.length) paragraphs.push(current.join(' '));
    current = [];
  };
  for (const raw of text.split('\n')) {
    let line = raw.trim();
    if (!line || /^(?:[-*_]\s*){3,}$/.test(line)) {
      flush();
      continue;
    }
    if (line.startsWith('|')) {
      const row = tableRow(line);
      if (row) current.push(row);
      continue;
    }
    const heading = /^#{1,6}\s+/.test(line);
    const listItem = /^(?:[-*+]|\d+[.)])\s+/.test(line);
    line = line
      .replace(/^>\s?/, '')
      .replace(/^#{1,6}\s+/, '')
      .replace(/^(?:[-*+]|\d+[.)])\s+(?:\[[ xX]\]\s+)?/, '')
      .replace(/(\*\*|__|~~)(.+?)\1/g, '$2')
      .replace(/(^|[^\w*])[*_]([^*_\n]+)[*_](?=[^\w*]|$)/g, '$1$2')
      .replace(/[*_~#>|`]+/g, ' ')
      .replace(/\s+/g, ' ')
      .replace(/\s+([.,;:!?])/g, '$1')
      .trim();
    if (!line) continue;
    if (heading || listItem) {
      flush();
      current.push(closeSentence(line));
      flush();
    } else {
      current.push(line);
    }
  }
  flush();
  return paragraphs.join('\n\n');
}

/** Whether the full stop ending `before` closes a sentence rather than an abbreviation or initial. */
function closesSentence(before: string): boolean {
  const word = /([A-Za-z.]+)$/.exec(before)?.[1]?.toLowerCase() ?? '';
  return !(ABBREVIATIONS.has(word) || /^(?:[a-z]\.)*[a-z]$/.test(word));
}

/** The sentences of plain `text`, in order. */
export function sentences(text: string): string[] {
  const result: string[] = [];
  for (const paragraph of text.split(/\n{2,}/)) {
    let start = 0;
    for (const match of paragraph.matchAll(/([.!?…]+)(["'”’)\]]*)(?=\s|$)/g)) {
      const marks = match[1];
      if (marks === '.' && !closesSentence(paragraph.slice(0, match.index))) continue;
      const end = match.index + match[0].length;
      const sentence = paragraph.slice(start, end).trim();
      if (sentence) result.push(sentence);
      start = end;
    }
    const rest = paragraph.slice(start).trim();
    if (rest) result.push(rest);
  }
  return result;
}

/** Breaks a sentence longer than `max` characters at clauses, then spaces. */
function splitLong(sentence: string, max: number): string[] {
  if (sentence.length <= max) return [sentence];
  const pieces: string[] = [];
  let rest = sentence;
  while (rest.length > max) {
    const window = rest.slice(0, max);
    const clause = Math.max(window.lastIndexOf(', '), window.lastIndexOf('; '), window.lastIndexOf(': '), window.lastIndexOf(' — '));
    const space = window.lastIndexOf(' ');
    const cut = clause > max / 3 ? clause + 1 : space > 0 ? space : max;
    pieces.push(rest.slice(0, cut).trim());
    rest = rest.slice(cut).trim();
  }
  if (rest) pieces.push(rest);
  return pieces;
}

export interface ChunkOptions {
  /** The longest piece sent to the voice in one call. */
  max?: number;
  /** Pieces after the first gather sentences up to this length. */
  target?: number;
}

/**
 * The pieces read-aloud asks the voice for, in order: the first sentence on
 * its own, then sentences gathered up to `target` characters.
 */
export function speechChunks(markdown: string, { max = 300, target = 220 }: ChunkOptions = {}): string[] {
  const pieces = sentences(speakableText(markdown)).flatMap((sentence) => splitLong(sentence, max));
  const chunks: string[] = [];
  for (const piece of pieces) {
    const last = chunks.length >= 2 ? chunks[chunks.length - 1] : null;
    if (last !== null && last.length + piece.length + 1 <= target) {
      chunks[chunks.length - 1] = `${last} ${piece}`;
    } else {
      chunks.push(piece);
    }
  }
  return chunks;
}
