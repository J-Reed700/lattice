import { describe, expect, it } from 'vitest';

import { passageKey, passageNumbersFor } from '@/features/learning/model/evidencePresentation';
import type { LearningClaimEvidenceDto, LearningEvidencePassageDto } from '@/lib/bindings';

const passage = (version: string, startByte = 0): LearningEvidencePassageDto => ({
  sourceVersionId: version, startByte, endByte: startByte + 5,
  title: 'Source', text: 'Quote', url: null, retrievalKind: 'lexical',
});
const claim = (sectionIndex: number, passages: LearningEvidencePassageDto[]): LearningClaimEvidenceDto => ({
  sectionIndex, passages, claim: 'Claim', contentQuote: 'Lesson text', verdict: 'supported', reason: 'Reason', supportingQuote: null,
});

describe('lesson passage numbering', () => {
  it('deduplicates the same saved passage across sections without collapsing different versions or spans', () => {
    const first = passage('v1');
    const second = passage('v1', 10);
    const third = passage('v2');
    const numbers = passageNumbersFor({ teachingClaims: [claim(0, [first, second]), claim(1, [first, third])] });
    expect([...numbers.entries()]).toEqual([[passageKey(first), 1], [passageKey(second), 2], [passageKey(third), 3]]);
  });

  it('has no citation numbers before a report is available', () => {
    expect(passageNumbersFor(null).size).toBe(0);
    expect(passageNumbersFor(undefined).size).toBe(0);
  });
});
