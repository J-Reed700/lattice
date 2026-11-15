import type { ReplyPieceDto, ReplyReferenceDto } from '@/lib/bindings';
import { normalizeAssistantMarkdown } from '@/utils/assistantMarkdown';
import type { FoundMark } from '@/utils/foundMark';

export type ReplyPiece = ReplyPieceDto;
export type ReplyReference = ReplyReferenceDto;

/**
 * A source reference travels through markdown as private-use characters: a
 * start, the index of the reference among the reply's source references in
 * decimal digits (each its own private-use character), an end. The viewer
 * draws a chip over it; nothing reads the reply's own text to find it.
 */
const MARK_START = '\uE000';
const MARK_END = '\uE00B';
const DIGIT_BASE = 0xe001;

export const MARK = /\uE000([\uE001-\uE00A]+)\uE00B/g;

export function markFor(index: number): string {
  const digits = Array.from(String(index), (digit) => String.fromCharCode(DIGIT_BASE + Number(digit)));
  return `${MARK_START}${digits.join('')}${MARK_END}`;
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

/** What a part adds to the message's own text: the words, or the handle as written. */
export function writtenOf(part: ReplyPiece): string {
  return part.type === 'text' ? part.text : part.reference.written;
}

/** The reply as markdown with a mark in place of each source reference, and the number each mark stands for. */
export function markReply(reply: readonly ReplyPiece[]): { raw: string; numbers: number[] } {
  const numbers: number[] = [];
  let raw = '';
  for (const piece of reply) {
    if (piece.type === 'text') {
      raw += piece.text;
    } else if (piece.reference.kind === 'source') {
      raw += markFor(numbers.length);
      numbers.push(piece.reference.number);
    } else {
      raw += piece.reference.written;
    }
  }
  return { raw, numbers };
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

/** Text of a drawn reply with each mark written as its number in brackets. */
export function writeMarksAsNumbers(text: string, numbers: readonly number[]): string {
  return text.replace(MARK, (_mark, captured: string) => {
    const number = numbers[markIndex(captured)];
    return number === undefined ? '' : `[${number}]`;
  });
}

export interface DrawnReply {
  /** For the viewer: marks stand where source references are. */
  markdown: string;
  /** For copy, export and reading aloud: each source reference as its number. */
  readable: string;
  /** The source number of each mark, by the index the mark carries. */
  numbers: number[];
}

export function drawReply(reply: readonly ReplyPiece[]): DrawnReply {
  const { raw, numbers } = markReply(reply);
  const markdown = normalizeAssistantMarkdown(raw);
  return { markdown, readable: writeMarksAsNumbers(markdown, numbers), numbers };
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
    const text = writtenOf(piece);
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
