import { describe, expect, it } from 'vitest';

import { verificationSummaryLine } from '@/features/chat/model/verificationSummary';

describe('verificationSummaryLine', () => {
  it('never reports a count of zero unverified claims', () => {
    const line = verificationSummaryLine(4, 0);
    expect(line).toBe('Every claim is grounded in your sources · 4 claims checked');
    expect(line).not.toContain('Unverified');
    expect(line).not.toContain('0');
  });

  it('says how many claims fell through when some did', () => {
    expect(verificationSummaryLine(4, 1)).toBe(
      '1 of 4 claims could not be grounded in your sources'
    );
  });

  it('counts a single claim in the singular', () => {
    expect(verificationSummaryLine(1, 0)).toBe(
      'Every claim is grounded in your sources · 1 claim checked'
    );
  });
});

describe('verificationSummaryLine with unchecked claims', () => {
  it('does not count an unchecked claim as ungrounded', () => {
    expect(verificationSummaryLine(5, 0, 2)).toBe(
      'Every checked claim is grounded in your sources · 3 of 5 claims checked'
    );
  });

  it('names unchecked claims beside the ungrounded ones', () => {
    expect(verificationSummaryLine(5, 1, 2)).toBe(
      '1 of 5 claims could not be grounded in your sources · 2 not checked'
    );
  });
});
