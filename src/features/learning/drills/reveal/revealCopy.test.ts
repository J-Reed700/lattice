import { describe, expect, it } from 'vitest';

import { durationText, joinList, lineLabel, momentOf, percentText, plural } from '@/features/learning/drills/reveal/revealCopy';
import { GRADED_AT } from '@/features/learning/drills/reveal/revealTestData';

describe('revealCopy', () => {
  it('joins a list as a sentence does', () => {
    expect(joinList([])).toBe('');
    expect(joinList(['a'])).toBe('a');
    expect(joinList(['a', 'b'])).toBe('a and b');
    expect(joinList(['a', 'b', 'c'])).toBe('a, b and c');
  });

  it('counts things', () => {
    expect(plural(1, 'issue')).toBe('1 issue');
    expect(plural(3, 'issue')).toBe('3 issues');
    expect(plural(0, 'issue')).toBe('0 issues');
    expect(plural(2, 'person', 'people')).toBe('2 people');
  });

  it('writes a share as a whole percent', () => {
    expect(percentText(0)).toBe('0%');
    expect(percentText(0.625)).toBe('63%');
    expect(percentText(1)).toBe('100%');
  });

  it('writes a duration in its two largest units', () => {
    expect(durationText(0)).toBe('0 s');
    expect(durationText(42_000)).toBe('42 s');
    expect(durationText(60_000)).toBe('1 min');
    expect(durationText(90_000)).toBe('1 min 30 s');
    expect(durationText(3_600_000)).toBe('1 h');
    expect(durationText(3_900_000)).toBe('1 h 5 min');
    expect(durationText(-5_000)).toBe('0 s');
  });

  it('gives a moment as the reader’s text and as an ISO time, and none for a number that is no time', () => {
    const moment = momentOf(GRADED_AT);
    expect(moment?.iso).toBe('2026-11-04T15:30:00.000Z');
    expect(moment?.text).toBeTruthy();
    expect(momentOf(Number.NaN)).toBeNull();
    expect(momentOf(8.64e15 + 1)).toBeNull();
  });

  it('names the lines of a span', () => {
    expect(lineLabel({ path: 'src/a.py', start: 3, end: 3 })).toBe('src/a.py · line 3');
    expect(lineLabel({ path: 'src/a.py', start: 3, end: 5 })).toBe('src/a.py · lines 3–5');
  });

  it('names the sentences of a span of an explanation, with no file', () => {
    expect(lineLabel({ path: 'explanation.md', start: 4, end: 4 }, 'passage')).toBe('Sentence 4');
    expect(lineLabel({ path: 'explanation.md', start: 4, end: 6 }, 'passage')).toBe('Sentences 4–6');
  });
});
