import { describe, expect, it } from 'vitest';

import type { SupportDto } from '@/lib/bindings';
import { cites, filePiece, said } from '@/tests/replyFixtures';
import type { ReplyPiece } from '@/utils/replyPieces';

import { anchoredOccurrences } from '../occurrences';

const restates: SupportDto = { relation: 'restates', anchors: [{ exact: 'Tea is hot.', span: { start: 4, end: 15 } }] };
const quotes: SupportDto = { relation: 'quotes', anchors: [{ exact: 'Kettles boil water.' }] };

const withSupport = (number: number, support: SupportDto): ReplyPiece => {
  const piece = cites(number);
  if (piece.type !== 'reference') throw new Error('a reference piece was expected');
  return { type: 'reference', reference: { ...piece.reference, support } };
};

describe('anchoredOccurrences', () => {
  it('lists each supported chip of a source by its place among that source chips', () => {
    const found = anchoredOccurrences([
      said('One '),
      withSupport(2, restates),
      said('. Two '),
      withSupport(3, quotes),
      said('. Three '),
      withSupport(2, quotes),
    ]);

    expect(found.get(2)).toEqual([
      { occurrence: 0, relation: 'restates', anchors: restates.anchors },
      { occurrence: 1, relation: 'quotes', anchors: quotes.anchors },
    ]);
    expect(found.get(3)).toEqual([{ occurrence: 0, relation: 'quotes', anchors: quotes.anchors }]);
  });

  it('gives a chip without support no entry but counts it', () => {
    const found = anchoredOccurrences([cites(2), said(' and '), withSupport(2, restates), said(' and '), cites(2)]);

    expect(found.get(2)).toEqual([{ occurrence: 1, relation: 'restates', anchors: restates.anchors }]);
  });

  it('counts neither file references nor text', () => {
    const found = anchoredOccurrences([
      withSupport(2, quotes),
      filePiece(2, 'grid.rs', 4, 9),
      said(' between '),
      withSupport(2, restates),
    ]);

    expect(found.get(2)?.map((entry) => entry.occurrence)).toEqual([0, 1]);
  });

  it('has nothing for a reply without support', () => {
    expect(anchoredOccurrences([said('Plain '), cites(1)]).size).toBe(0);
  });
});
