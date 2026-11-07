import { describe, expect, it } from 'vitest';

import {
  cleanLinesOf,
  contrastPaths,
  issuesNamed,
  leadingLine,
  matchLabel,
  numberOf,
  rankIssues,
  suiteSummary,
} from '@/features/learning/drills/reveal/revealPresentation';
import {
  CLEAN_FILES,
  FINDINGS,
  ISSUES,
  MAGIC_NUMBER,
  REVIEWED_FILES,
  STALE_STATE,
  UNBOUNDED_GROWTH,
  WRONG_OPERATOR,
  revealOf,
} from '@/features/learning/drills/reveal/revealTestData';
import type { CodeReviewFileDto, DiffLine, RevealedIssueDto } from '@/lib/bindings';

const ranked = rankIssues(ISSUES);

describe('rankIssues', () => {
  it('puts the most severe issue first and numbers the issues as the page shows them', () => {
    expect(ranked.map(({ issue, position, number }) => [issue.patternName, position, number])).toEqual([
      ['Stale state', 2, 1],
      ['Wrong operator or comparison', 1, 2],
      ['Magic number', 0, 3],
    ]);
  });

  it('keeps the key’s order among issues of one severity', () => {
    const first: RevealedIssueDto = { ...WRONG_OPERATOR, id: 'first' };
    const second: RevealedIssueDto = { ...WRONG_OPERATOR, id: 'second' };
    expect(rankIssues([second, first]).map((entry) => entry.issue.id)).toEqual(['second', 'first']);
  });

  it('puts the planted issues before the also-valid ones, whatever their severity', () => {
    const mixed = rankIssues([UNBOUNDED_GROWTH, ...ISSUES]);
    expect(mixed.map(({ issue, position, number }) => [issue.patternName, position, number])).toEqual([
      ['Stale state', 3, 1],
      ['Wrong operator or comparison', 2, 2],
      ['Magic number', 1, 3],
      ['Unbounded growth', 0, 4],
    ]);
  });

  it('ranks the also-valid issues most severe first, and by the key’s order among equals', () => {
    const minor: RevealedIssueDto = { ...UNBOUNDED_GROWTH, id: 'minor', severity: 'minor' };
    const same: RevealedIssueDto = { ...UNBOUNDED_GROWTH, id: 'same' };
    expect(rankIssues([minor, UNBOUNDED_GROWTH, same]).map((entry) => entry.issue.id)).toEqual([
      'review-1',
      'same',
      'minor',
    ]);
  });

  it('leaves the issues it is given as they were', () => {
    const given = [...ISSUES];
    rankIssues(given);
    expect(given).toEqual(ISSUES);
  });

  it('finds an issue’s number by its position in the key', () => {
    expect(numberOf(2, ranked)).toBe(1);
    expect(numberOf(0, ranked)).toBe(3);
    expect(numberOf(9, ranked)).toBe(Number.MAX_SAFE_INTEGER);
  });
});

describe('matchLabel', () => {
  it('names the issue a comment matches by the number and the pattern the page gives it', () => {
    expect(matchLabel(FINDINGS[0], ranked)).toBe('Matches issue 2, Wrong operator or comparison');
    expect(matchLabel(FINDINGS[1], ranked)).toBe('Matches issue 1, Stale state');
  });

  it('names every issue a comment matches', () => {
    const both = { ...FINDINGS[0], issues: [1, 2] };
    expect(matchLabel(both, ranked)).toBe('Matches issue 2, Wrong operator or comparison and issue 1, Stale state');
  });

  it('says what it can when the issues are not there', () => {
    expect(matchLabel({ ...FINDINGS[0], issues: [] }, ranked)).toBe('Matches the key');
    expect(matchLabel({ ...FINDINGS[0], issues: [7] }, ranked)).toBe('Matches issue 8');
  });
});

describe('issuesNamed', () => {
  it('lists issues in the page’s order with their numbers', () => {
    expect(issuesNamed([0, 2], ranked)).toBe('Stale state (issue 1) and Magic number (issue 3)');
    expect(issuesNamed([1], ranked)).toBe('Wrong operator or comparison (issue 2)');
    expect(issuesNamed([], ranked)).toBe('');
  });

  it('keeps an issue the page does not have, last', () => {
    expect(issuesNamed([5, 1], ranked)).toBe('Wrong operator or comparison (issue 2) and issue 6');
  });
});

describe('leadingLine', () => {
  const ledScore = { ...revealOf().score, ledWithTheMostSevere: true };
  const top = (patch: Partial<RevealedIssueDto>): RevealedIssueDto => ({ ...STALE_STATE, ...patch });

  it('says the review led, and with what', () => {
    const reveal = revealOf({ issues: [top({ outcome: 'found', finding: 0 }), WRONG_OPERATOR], score: ledScore });
    expect(leadingLine(reveal)).toEqual({
      led: true,
      text: 'Led with the most severe issue: your first comment found Stale state.',
    });
  });

  it('says the review led even when the issues do not say by what', () => {
    expect(leadingLine(revealOf({ score: ledScore }))).toEqual({ led: true, text: 'Led with the most severe issue.' });
  });

  it('says why a review without comments did not lead', () => {
    expect(leadingLine(revealOf({ findings: [] }))).toEqual({
      led: false,
      text: 'Did not lead with the most severe issue: you left no comments.',
    });
  });

  it('says the most severe issue was found, but by a later comment', () => {
    const reveal = revealOf({ issues: [top({ outcome: 'found', finding: 2 })] });
    expect(leadingLine(reveal)?.text).toBe(
      'Did not lead with the most severe issue: Stale state was found by your comment 3, not your first.',
    );
  });

  it('names the earliest comment when several of the most severe issues were found late', () => {
    const reveal = revealOf({
      issues: [
        top({ id: 'a', patternName: 'A', outcome: 'found', finding: 3 }),
        top({ id: 'b', patternName: 'B', outcome: 'found', finding: 1 }),
      ],
    });
    expect(leadingLine(reveal)?.text).toBe(
      'Did not lead with the most severe issue: A and B were found by your comment 2, not your first.',
    );
  });

  it('says the most severe issue was only half found', () => {
    expect(leadingLine(revealOf())).toEqual({
      led: false,
      text: 'Did not lead with the most severe issue: Stale state was only half found: a comment pointed at it but gave no cause, or the wrong one.',
    });
  });

  it('says the most severe issue was missed', () => {
    const reveal = revealOf({ issues: [top({ outcome: 'missed', finding: null }), MAGIC_NUMBER] });
    expect(leadingLine(reveal)).toEqual({
      led: false,
      text: 'Did not lead with the most severe issue: you missed Stale state.',
    });
  });

  it('judges by the most severe issues the key has, whichever severity that is', () => {
    const reveal = revealOf({ issues: [MAGIC_NUMBER] });
    expect(leadingLine(reveal)?.text).toBe('Did not lead with the most severe issue: you missed Magic number.');
  });

  it('judges by the planted issues alone: a missed also-valid issue above them is not the top', () => {
    const missedOperator: RevealedIssueDto = { ...WRONG_OPERATOR, outcome: 'missed', finding: null };
    const reveal = revealOf({ issues: [missedOperator, UNBOUNDED_GROWTH] });
    expect(leadingLine(reveal)?.text).toBe(
      'Did not lead with the most severe issue: you missed Wrong operator or comparison.',
    );
  });

  it('says the review led when the first comment found an also-valid issue above the planted top', () => {
    const raised: RevealedIssueDto = { ...UNBOUNDED_GROWTH, outcome: 'found', finding: 0 };
    const later: RevealedIssueDto = { ...WRONG_OPERATOR, finding: 1 };
    const reveal = revealOf({ issues: [later, raised], score: ledScore });
    expect(leadingLine(reveal)).toEqual({
      led: true,
      text: 'Led with the most severe issue: your first comment found Unbounded growth.',
    });
  });

  it('names a planted top found first even when an also-valid issue above it was not raised', () => {
    const reveal = revealOf({
      issues: [top({ outcome: 'found', finding: 0 }), { ...UNBOUNDED_GROWTH, severity: 'critical' }],
      score: ledScore,
    });
    expect(leadingLine(reveal)?.text).toBe(
      'Led with the most severe issue: your first comment found Stale state.',
    );
  });

  it('does not say the review led when the first comment found an also-valid issue below the planted top', () => {
    const raised: RevealedIssueDto = { ...UNBOUNDED_GROWTH, severity: 'nit', outcome: 'found', finding: 0 };
    const reveal = revealOf({ issues: [STALE_STATE, raised] });
    expect(leadingLine(reveal)?.led).toBe(false);
  });

  it('has nothing to say about a key without issues', () => {
    expect(leadingLine(revealOf({ issues: [] }))).toBeNull();
  });
});

describe('cleanLinesOf', () => {
  const file = (text: string): CodeReviewFileDto => ({ path: 'a.py', text });
  const fixing = (lines: DiffLine[], start = 1): RevealedIssueDto => ({
    ...STALE_STATE,
    span: { path: 'a.py', start, end: start },
    fix: { path: 'a.py', lines },
  });

  it('finds the lines a fix leaves in the clean code, wherever they sit', () => {
    expect(cleanLinesOf(WRONG_OPERATOR, CLEAN_FILES)).toEqual({ startLine: 13, endLine: 14 });
    expect(cleanLinesOf(MAGIC_NUMBER, CLEAN_FILES)).toEqual({ startLine: 8, endLine: 8 });
    const shifted = [file('top\nof\nthe\nfile\nb\nc\n')];
    const lines: DiffLine[] = [
      { kind: 'removed', text: 'bad' },
      { kind: 'added', text: 'b' },
      { kind: 'unchanged', text: 'c' },
    ];
    expect(cleanLinesOf(fixing(lines, 2), shifted)).toEqual({ startLine: 5, endLine: 6 });
  });

  it('takes the run nearest the issue when the lines repeat', () => {
    const repeated = [file('a\nb\nc\nb\n')];
    const added: DiffLine[] = [{ kind: 'added', text: 'b' }];
    expect(cleanLinesOf(fixing(added, 1), repeated)).toEqual({ startLine: 2, endLine: 2 });
    expect(cleanLinesOf(fixing(added, 4), repeated)).toEqual({ startLine: 4, endLine: 4 });
  });

  it('reads windows line endings', () => {
    const added: DiffLine[] = [{ kind: 'added', text: 'b' }];
    expect(cleanLinesOf(fixing(added), [file('a\r\nb\r\n')])).toEqual({ startLine: 2, endLine: 2 });
  });

  it('finds nothing when the clean code lacks the file or the lines, or the fix only removes lines', () => {
    const added: DiffLine[] = [{ kind: 'added', text: 'b' }];
    expect(cleanLinesOf(fixing(added), [])).toBeNull();
    expect(cleanLinesOf(fixing(added), [file('a\nc\n')])).toBeNull();
    expect(cleanLinesOf(fixing([{ kind: 'removed', text: 'bad' }]), [file('a\nb\n')])).toBeNull();
    expect(cleanLinesOf(fixing([]), [file('a\nb\n')])).toBeNull();
  });
});

describe('contrastPaths', () => {
  it('lists the reviewed files, then the files only the clean code has', () => {
    const extra: CodeReviewFileDto = { path: 'src/extra.py', text: '' };
    expect(contrastPaths(REVIEWED_FILES, [...CLEAN_FILES, extra])).toEqual(['src/limiter.py', 'src/config.py', 'src/extra.py']);
    expect(contrastPaths([], [])).toEqual([]);
  });
});

describe('suiteSummary', () => {
  it('says a passing suite passed, and what it ran with', () => {
    const alone = suiteSummary({ status: 'passed', ranBeside: [] }, ranked);
    expect(alone).toEqual({
      verdict: 'passed',
      headline: 'The hidden suite passes on your files: nothing that worked is broken.',
      detail: 'It ran on your files as you left them.',
      output: null,
    });
    expect(suiteSummary({ status: 'passed', ranBeside: [0, 2] }, ranked).detail).toBe(
      'It ran with the fixes of the issues you left: Stale state (issue 1) and Magic number (issue 3).',
    );
  });

  it('says a failing suite failed, with what it wrote and what it ran with', () => {
    expect(suiteSummary({ status: 'failed', output: 'AssertionError', ranBeside: [1] }, ranked)).toEqual({
      verdict: 'failed',
      headline: 'The hidden suite fails on your files: your changes broke something that worked.',
      detail: 'It ran with the fixes of the issues you left: Wrong operator or comparison (issue 2).',
      output: 'AssertionError',
    });
  });

  it('says why a suite did not run', () => {
    expect(suiteSummary({ status: 'not_run', why: 'This drill has no hidden suite.' }, ranked)).toEqual({
      verdict: 'not_run',
      headline: 'The hidden suite did not run.',
      detail: 'This drill has no hidden suite.',
      output: null,
    });
  });
});
