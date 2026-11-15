import { describe, expect, it } from 'vitest';

import { filePiece, reply, said } from '@/tests/replyFixtures';

import {
  drawReply,
  extendReply,
  fileMarkFor,
  findReplyMarks,
  markFor,
  markIndex,
  plainTextOf,
  replyOf,
  replyPlainText,
  writtenOf,
} from '../replyPieces';

describe('reply pieces', () => {
  it('reads an index back from its mark', () => {
    for (const index of [0, 7, 10, 305]) {
      const captured = markFor(index).slice(1, -1);
      expect(markIndex(captured)).toBe(index);
    }
  });

  it('takes a message without pieces as one piece of text', () => {
    expect(replyOf({ content: 'Plain words.' })).toEqual([said('Plain words.')]);
    expect(replyOf({ content: '' })).toEqual([]);
    expect(replyOf({ content: 'ignored', reply: reply('Kept ', 2, '.') })).toEqual(reply('Kept ', 2, '.'));
  });

  it('joins streamed text and keeps each reference on its own', () => {
    const grown = extendReply(reply('Kale ', 3), reply(' grows', 4, ' fast'));

    expect(grown).toEqual(reply('Kale ', 3, ' grows', 4, ' fast'));
    expect(extendReply(reply('One '), [said('two')])).toEqual([said('One two')]);
    expect(grown.map(writtenOf).join('')).toBe('Kale [s3] grows[s4] fast');
  });

  it('draws a source reference as a mark and the number it shows', () => {
    const drawn = drawReply(reply('Kale grows ', 3, ' and chard ', 5, '.'));

    expect(drawn.numbers).toEqual([3, 5]);
    expect(drawn.markdown).toBe(`Kale grows ${markFor(0)} and chard ${markFor(1)}.`);
    expect(drawn.readable).toBe('Kale grows [3] and chard [5].');
  });

  it('draws a file reference as inline code with the lines it names', () => {
    const drawn = drawReply([said('See '), filePiece(2, 'grid.rs', 4, 9), said(' and '), filePiece(3, 'a.rs', 7, 7), said('.')]);

    expect(drawn.numbers).toEqual([]);
    expect(drawn.files).toEqual([]);
    expect(drawn.markdown).toBe('See `grid.rs:4-9` and `a.rs:7`.');
    expect(plainTextOf({ content: '', reply: [filePiece(2, 'grid.rs', 4, 9)] })).toBe('grid.rs:4-9');
  });

  it('gives a folder reply file marks that read as the same text', () => {
    const drawn = drawReply([said('See '), filePiece(2, 'grid.rs', 4, 9), said('.')], true);

    expect(drawn.files).toEqual([{ name: 'grid.rs', lines: { first: 4, last: 9 } }]);
    expect(drawn.markdown).toBe(`See \`${fileMarkFor(0)}grid.rs:4-9\`.`);
    expect(drawn.readable).toBe('See `grid.rs:4-9`.');
  });

  it('leaves bracketed digits in text alone', () => {
    const drawn = drawReply(reply('Step [3] is plain ', 1, '.'));

    expect(drawn.numbers).toEqual([1]);
    expect(drawn.readable).toBe('Step [3] is plain [1].');
  });

  it('finds the marks of a drawn reply with the number each stands for', () => {
    const drawn = drawReply(reply('Kale ', 3, ' and chard ', 5, '.'));

    expect(findReplyMarks(drawn.markdown, drawn.numbers).map((mark) => mark.number)).toEqual([3, 5]);
    expect(findReplyMarks('Step [3] is plain.', drawn.numbers)).toEqual([]);
    expect(findReplyMarks(drawn.markdown, [])).toEqual([]);
  });

  it('writes a reply as plain text with its source references left out', () => {
    expect(replyPlainText(reply('Kale grows fast ', 3, '. Chard ', 5, ' bolts', 4, ', then dies.'))).toBe(
      'Kale grows fast. Chard bolts, then dies.',
    );
    expect(plainTextOf({ content: 'Spaced  out , as typed.' })).toBe('Spaced  out , as typed.');
    expect(plainTextOf({ content: 'stored [s3]', reply: reply('shown ', 3) })).toBe('shown ');
  });
});
