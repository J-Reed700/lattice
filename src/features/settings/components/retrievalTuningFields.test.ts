import { describe, expect, it } from 'vitest';

import { normalizeFieldValue, normalizeTuningPairBounds, TUNING_GROUPS } from './retrievalTuningFields';

const base = {
  docShortlistCandidateMin: 10, docShortlistCandidateMax: 40,
  docShortlistDocMin: 2, docShortlistDocMax: 8,
  docSupportMultiHitRatioMin: 0.2, docSupportMultiHitRatioMax: 0.6,
  docSupportSingleHitRatioMin: 0.1, docSupportSingleHitRatioMax: 0.5,
} as never;

describe('retrievalTuningFields', () => {
  it('groups every knob with sane bounds', () => {
    expect(TUNING_GROUPS.map((group) => group.title)).toContain('Candidates and shortlist');
    for (const field of TUNING_GROUPS.flatMap((group) => group.fields)) {
      expect(field.min).toBeLessThanOrEqual(field.max);
    }
  });

  it('clamps and rounds whole-number fields, and keeps two decimals for ratios', () => {
    const whole = { key: 'docShortlistDocMin', label: '', min: 1, max: 128, step: 1 } as const;
    const ratio = { key: 'docSupportMultiHitRatioMin', label: '', min: 0, max: 1, step: 0.01 } as const;
    expect(normalizeFieldValue(whole as never, 300)).toBe(128);
    expect(normalizeFieldValue(whole as never, 2.6)).toBe(3);
    expect(normalizeFieldValue(ratio as never, 0.12345)).toBe(0.12);
    expect(normalizeFieldValue(ratio as never, -1)).toBe(0);
  });

  it.each([
    ['docShortlistCandidateMin', { docShortlistCandidateMin: 50 }, { docShortlistCandidateMax: 50 }],
    ['docShortlistCandidateMax', { docShortlistCandidateMax: 5 }, { docShortlistCandidateMin: 5 }],
    ['docShortlistDocMin', { docShortlistDocMin: 9 }, { docShortlistDocMax: 9 }],
    ['docShortlistDocMax', { docShortlistDocMax: 1 }, { docShortlistDocMin: 1 }],
    ['docSupportMultiHitRatioMin', { docSupportMultiHitRatioMin: 0.9 }, { docSupportMultiHitRatioMax: 0.9 }],
    ['docSupportMultiHitRatioMax', { docSupportMultiHitRatioMax: 0.1 }, { docSupportMultiHitRatioMin: 0.1 }],
    ['docSupportSingleHitRatioMin', { docSupportSingleHitRatioMin: 0.7 }, { docSupportSingleHitRatioMax: 0.7 }],
    ['docSupportSingleHitRatioMax', { docSupportSingleHitRatioMax: 0.05 }, { docSupportSingleHitRatioMin: 0.05 }],
  ])('moves the other bound when %s crosses it', (key, change, expected) => {
    const next = normalizeTuningPairBounds(key as never, { ...(base as object), ...change } as never);
    expect(next).toMatchObject({ ...change, ...expected });
  });

  it('leaves bounds that do not cross', () => {
    expect(normalizeTuningPairBounds('docShortlistDocMin' as never, base)).toEqual(base);
    expect(normalizeTuningPairBounds('webSearchMaxResults' as never, base)).toEqual(base);
  });
});
