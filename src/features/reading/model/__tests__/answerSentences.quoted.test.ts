import { describe, expect, it } from 'vitest';

import { sentencesByOccurrence, sentencesCiting } from '@/features/reading/model/answerSentences';
import { cites, said } from '@/tests/replyFixtures';
import type { ReplyPiece } from '@/utils/replyPieces';

const ANSWER: ReplyPiece[] = [
  said('The guide says '),
  { type: 'quoted', text: 'Tea is hot and sweet' },
  said(' '),
  cites(2),
  said('.'),
];

describe('answer sentences with a quotation', () => {
  it('give a matcher the quoted words without the private-use marks', () => {
    expect(sentencesCiting(ANSWER, 2)).toEqual(['The guide says Tea is hot and sweet.']);
    expect(sentencesByOccurrence(ANSWER, 2)).toEqual(['The guide says Tea is hot and sweet.']);
  });
});
