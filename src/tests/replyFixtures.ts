import { textPiece, type ReplyPiece } from '@/utils/replyPieces';

/** A reply's text piece. */
export const said = (text: string): ReplyPiece => textPiece(text);

/** A reference to source `number`, as the backend sends one. */
export const cites = (number: number): ReplyPiece => ({
  type: 'reference',
  reference: { written: `[s${number}]`, kind: 'source', number },
});

/** A reference to lines of a file. */
export const filePiece = (number: number, name: string, first: number, last: number): ReplyPiece => ({
  type: 'reference',
  reference: { written: `[f${number}:${first}-${last}]`, kind: 'file', number, name, lines: { first, last } },
});

/** A reply written as alternating text and source numbers: `reply('A claim ', 4, '.')`. */
export const reply = (...parts: (string | number)[]): ReplyPiece[] =>
  parts.map((part) => (typeof part === 'string' ? said(part) : cites(part)));
