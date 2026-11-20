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

describe('verificationSummaryLine in hybrid cited-only mode', () => {
  it('qualifies a fully verified line when uncited parts may use general knowledge', () => {
    expect(verificationSummaryLine(3, 0, 0, true)).toBe(
      'All cited claims are grounded in your sources · 3 claims checked · uncited parts may use general knowledge'
    );
  });

  it('uses singular claim for one checked cited claim', () => {
    expect(verificationSummaryLine(1, 0, 0, true)).toBe(
      'All cited claims are grounded in your sources · 1 claim checked · uncited parts may use general knowledge'
    );
  });

  it('notes uncited general knowledge when some cited claims are unchecked', () => {
    expect(verificationSummaryLine(5, 0, 2, true)).toBe(
      'Every checked cited claim is grounded in your sources · 3 of 5 cited claims checked · uncited parts may use general knowledge'
    );
    expect(verificationSummaryLine(1, 0, 1, true)).toBe(
      'Every checked cited claim is grounded in your sources · 0 of 1 cited claim checked · uncited parts may use general knowledge'
    );
  });

  it('notes uncited general knowledge when some cited claims could not be grounded', () => {
    expect(verificationSummaryLine(5, 1, 2, true)).toBe(
      '1 of 5 cited claims could not be grounded in your sources · 2 not checked · uncited parts may use general knowledge'
    );
    expect(verificationSummaryLine(1, 1, 0, true)).toBe(
      '1 of 1 cited claim could not be grounded in your sources · uncited parts may use general knowledge'
    );
  });
});
