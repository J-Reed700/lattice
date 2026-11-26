import type { LinesDto, ReplyPieceDto, ReplyReferenceDto } from '@/lib/bindings';
import { normalizeAssistantMarkdown } from '@/utils/assistantMarkdown';
import type { FoundMark } from '@/utils/foundMark';

export type ReplyPiece = ReplyPieceDto;
export type ReplyReference = ReplyReferenceDto;

/**
 * A source reference travels through markdown as private-use characters: a
 * start, the index of the reference among the reply's source references in
 * decimal digits (each its own private-use character), an end. The viewer
 * draws a chip over it; nothing reads the reply's own text to find it. A file
 * reference travels the same way with its own start and end, in front of the
 * text it is shown as.
 */
const MARK_START = '\uE000';
const MARK_END = '\uE00B';
const FILE_START = '\uE00C';
const FILE_END = '\uE00D';
const DIGIT_BASE = 0xe001;

/** A quotation travels between its own pair of private-use characters, the two after the reference marks. */
export const QUOTE_START = '\uE00E';
export const QUOTE_END = '\uE00F';

export const MARK = /\uE000([\uE001-\uE00A]+)\uE00B/g;
export const FILE_MARK = /\uE00C([\uE001-\uE00A]+)\uE00D/g;
const ANY_MARK = /[\uE000-\uE00F]/g;

function digitsOf(index: number): string {
  return Array.from(String(index), (digit) => String.fromCharCode(DIGIT_BASE + Number(digit))).join('');
}

export function markFor(index: number): string {
  return `${MARK_START}${digitsOf(index)}${MARK_END}`;
}

export function fileMarkFor(index: number): string {
  return `${FILE_START}${digitsOf(index)}${FILE_END}`;
}

/** A file a reply points at, and the lines of it when it names some. */
export interface FileTarget {
  name: string;
  lines: LinesDto | null;
}

/** How a file reference reads: `name:first-last`, `name:first`, or `name` alone. */
export function fileLabel({ name, lines }: FileTarget): string {
  if (!lines) return name;
  return lines.first === lines.last ? `${name}:${lines.first}` : `${name}:${lines.first}-${lines.last}`;
}

function fileOf({ name, lines, written }: ReplyReference): FileTarget {
  return { name: name ?? written, lines: lines ?? null };
}

function inlineCode(text: string): string {
  const fence = '`'.repeat(Math.max(0, ...Array.from(text.matchAll(/`+/g), (run) => run[0].length)) + 1);
  return `${fence}${text}${fence}`;
}

/** The index a match of `MARK` stands for, from its captured digits. */
export function markIndex(captured: string | undefined): number {
  const digits = Array.from(captured ?? '', (char) => String(char.charCodeAt(0) - DIGIT_BASE));
  return Number(digits.join(''));
}

export function textPiece(text: string): ReplyPiece {
  return { type: 'text', text };
}

/** The pieces of a message that has no list of its own: its text, whole. */
export function replyOf(message: { content: string; reply?: readonly ReplyPiece[] | null }): readonly ReplyPiece[] {
  if (message.reply) return message.reply;
  return message.content ? [textPiece(message.content)] : [];
}

/** New pieces after streamed parts arrive: text joins the last text piece, a reference stands alone. */
export function extendReply(reply: readonly ReplyPiece[], parts: readonly ReplyPiece[]): ReplyPiece[] {
  const next = [...reply];
  for (const part of parts) {
    const last = next[next.length - 1];
    if (part.type === 'text' && last?.type === 'text') {
      next[next.length - 1] = textPiece(last.text + part.text);
    } else {
      next.push(part);
    }
  }
  return next;
}

/** A quotation as it reads in plain text: between curly quotes. */
function curly(text: string): string {
  return `“${text}”`;
}

/** What a part adds to the message's own text: the words, or the handle as written. */
export function writtenOf(part: ReplyPiece): string {
  if (part.type === 'reference') return part.reference.written;
  return part.type === 'quoted' ? curly(part.text) : part.text;
}

/**
 * The reply as markdown with a mark in place of each source reference, and the number each mark stands for.
 * A file reference is inline code; in a folder's reply (`folder`) it carries a mark the viewer draws a chip from.
 */
export function markReply(
  reply: readonly ReplyPiece[],
  folder = false,
): { raw: string; numbers: number[]; files: FileTarget[] } {
  const numbers: number[] = [];
  const files: FileTarget[] = [];
  let raw = '';
  for (const piece of reply) {
    if (piece.type === 'text') {
      raw += piece.text;
    } else if (piece.type === 'quoted') {
      raw += `${QUOTE_START}${piece.text}${QUOTE_END}`;
    } else if (piece.reference.kind === 'source') {
      raw += markFor(numbers.length);
      numbers.push(piece.reference.number);
    } else {
      const file = fileOf(piece.reference);
      raw += inlineCode(`${folder ? fileMarkFor(files.length) : ''}${fileLabel(file)}`);
      if (folder) files.push(file);
    }
  }
  return { raw, numbers, files };
}

/** Text read out of a drawn viewer with the marks' private-use characters left out. */
export function withoutMarks(text: string): string {
  return text.replace(ANY_MARK, '');
}

/** The marks of a drawn reply found in a run of its text; `numbers` is the list `drawReply` gave. */
export function findReplyMarks(text: string, numbers: readonly number[]): FoundMark[] {
  if (numbers.length === 0) return [];
  const found: FoundMark[] = [];
  for (const match of text.matchAll(MARK)) {
    const number = numbers[markIndex(match[1])];
    if (number !== undefined) found.push({ index: match.index ?? 0, length: match[0].length, number, glyph: true });
  }
  return found;
}

/** Text of a drawn reply with each source mark written as its number in brackets and each file mark dropped. */
export function writeMarksAsNumbers(text: string, numbers: readonly number[]): string {
  return text
    .replace(MARK, (_mark, captured: string) => {
      const number = numbers[markIndex(captured)];
      return number === undefined ? '' : `[${number}]`;
    })
    .replace(FILE_MARK, '')
    .replace(new RegExp(QUOTE_START, 'g'), '“')
    .replace(new RegExp(QUOTE_END, 'g'), '”');
}

export interface DrawnReply {
  /** For the viewer: marks stand where source references are. */
  markdown: string;
  /** For copy, export and reading aloud: each source reference as its number. */
  readable: string;
  /** The source number of each mark, by the index the mark carries. */
  numbers: number[];
  /** The file of each file mark, by the index the mark carries; empty unless the reply is a folder's. */
  files: FileTarget[];
}

export function drawReply(reply: readonly ReplyPiece[], folder = false): DrawnReply {
  const { raw, numbers, files } = markReply(reply, folder);
  const markdown = normalizeAssistantMarkdown(raw);
  return { markdown, readable: writeMarksAsNumbers(markdown, numbers), numbers, files };
}

function writtenAsText(piece: ReplyPiece): string {
  if (piece.type === 'reference') return fileLabel(fileOf(piece.reference));
  return piece.type === 'quoted' ? curly(piece.text) : piece.text;
}

/**
 * A reply's plain text for a surface that shows it as a line of text (a
 * preview, a title, what is spoken): each source reference is left out, with
 * the space it sat after. Surfaces that copy or export an answer use
 * `drawReply(...).readable`, which keeps each reference as its number.
 */
export function replyPlainText(reply: readonly ReplyPiece[]): string {
  let out = '';
  let dropped = false;
  for (const piece of reply) {
    if (piece.type === 'reference' && piece.reference.kind === 'source') {
      dropped = true;
      continue;
    }
    const text = writtenAsText(piece);
    if (dropped && /^[\s.,;:!?]/.test(text)) out = out.replace(/[ \t]+$/, '');
    dropped = false;
    out += text;
  }
  return out;
}

/** The plain text of a message that may or may not have pieces; a message without them is its content. */
export function plainTextOf(message: { content: string; reply?: readonly ReplyPiece[] | null }): string {
  return message.reply ? replyPlainText(message.reply) : message.content;
}
