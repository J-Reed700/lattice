import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { DrillReveal } from '@/features/learning/drills/reveal/DrillReveal';
import { gradedResult, revealOf } from '@/features/learning/drills/reveal/revealTestData';
import { quotePieces } from '@/features/learning/drills/reveal/sourceQuotes';
import type { CodeReviewResultDto, IssueFix, RevealedIssueDto } from '@/lib/bindings';

const PLANTED = 'It helped by 40% a week later.';
const CLEAN = 'It helped by 14% a week later.';
const REVIEWED = `Spacing helps recall.\n${PLANTED}\n\nThe benefit was smaller for known material.\n`;
const CLEANED = `Spacing helps recall.\n${CLEAN}\n\nThe benefit was smaller for known material.\n`;
const QUOTE = 'recalled 14% more a week later';
const SOURCE_TEXT = `Students who spaced their study ${QUOTE}.\n\nThe effect was smaller for what they already knew.`;

const ISSUE: RevealedIssueDto = {
  id: 'planted-1',
  pattern: 'fidelity.misquoted_figure',
  patternName: 'Misquoted figure',
  severity: 'major',
  span: { path: 'explanation.md', start: 2, end: 2 },
  consequence: 'A reader would expect nearly three times the gain.',
  explanation: 'The study found 14%, not 40%.',
  fix: {
    path: 'explanation.md',
    lines: [
      { kind: 'removed', text: PLANTED },
      { kind: 'added', text: CLEAN },
    ],
  },
  sourceQuote: QUOTE,
  outcome: 'found',
  finding: 0,
  reason: 'Your comment names the figure.',
};

const FIX: IssueFix = {
  issue: 0,
  outcome: 'fixed',
  decidedBy: 'source',
  reason: 'The source supports what you wrote.',
  output: null,
  fallback: null,
  gradedBeside: [],
  suite: { status: 'not_run', why: 'This drill has no hidden suite.' },
};

function passage(patch: Partial<CodeReviewResultDto> = {}): CodeReviewResultDto {
  const base = gradedResult();
  return {
    ...base,
    drill: {
      title: 'Why spacing helps',
      material: { kind: 'passage', sourceTitle: 'Spacing and memory' },
      files: [{ path: 'explanation.md', text: REVIEWED }],
    },
    findings: [{ span: { path: 'explanation.md', start: 2, end: 2 }, text: 'It is 14%.', severity: null }],
    reveal: revealOf({
      issues: [ISSUE],
      findings: [
        {
          span: { path: 'explanation.md', start: 2, end: 2 },
          text: 'It is 14%.',
          severity: null,
          outcome: 'matches',
          issues: [0],
          reason: 'It is the figure.',
        },
      ],
      score: {
        found: 1,
        halfFound: 0,
        missed: 0,
        validExtras: 0,
        falsePositives: 0,
        bySeverity: [{ severity: 'major', found: 1, halfFound: 0, total: 1 }],
        weightedRecall: 1,
        precision: 1,
        ledWithTheMostSevere: true,
      },
      ratings: [],
      fix: [FIX],
      cleanFiles: [{ path: 'explanation.md', text: CLEANED }],
      source: { title: 'Spacing and memory', text: SOURCE_TEXT, versionId: 'spacing-v1' },
    }),
    ...patch,
  };
}

function setup(result = passage()) {
  return render(<DrillReveal result={result} onNextDrill={vi.fn()} onRegrade={vi.fn()} regrading={false} />);
}

const pane = (name: string) => screen.getByRole('group', { name });

afterEach(() => {
  Reflect.deleteProperty(Element.prototype, 'scrollIntoView');
});

describe('quotePieces', () => {
  it('marks each quote where it stands in the passage', () => {
    expect(quotePieces('One. Two. Three.', ['Two.'])).toEqual([
      { text: 'One. ', quoted: false, focused: false },
      { text: 'Two.', quoted: true, focused: false },
      { text: ' Three.', quoted: false, focused: false },
    ]);
  });

  it('joins quotes that overlap, focused if any of them is, and leaves out one the passage does not hold', () => {
    const pieces = quotePieces('abcdefgh', ['bcde', 'defg', 'zzz', ''], 'defg');
    expect(pieces).toEqual([
      { text: 'a', quoted: false, focused: false },
      { text: 'bcdefg', quoted: true, focused: true },
      { text: 'h', quoted: false, focused: false },
    ]);
    expect(quotePieces('abc', [])).toEqual([{ text: 'abc', quoted: false, focused: false }]);
    expect(quotePieces('', ['a'])).toEqual([]);
  });
});

describe('DrillReveal, a graded review of an explanation', () => {
  it('counts the issues in the explanation', () => {
    setup();
    expect(screen.getByRole('heading', { name: 'Of the 1 issue in the explanation' })).toBeInTheDocument();
  });

  it('shows what the source says on the issue, with the sentence it is in named as a sentence', () => {
    setup();
    const card = within(screen.getByRole('article', { name: 'Misquoted figure' }));
    expect(card.getByText('What the source says')).toBeInTheDocument();
    expect(card.getByText(QUOTE)).toBeInTheDocument();
    expect(card.getByRole('button', { name: 'Show Sentence 2 in the explanation' })).toHaveTextContent('Sentence 2');
  });

  it('gives the two explanations a heading of their own and marks the planted sentence and its repair', () => {
    setup();
    expect(
      screen.getByRole('heading', { level: 2, name: 'The explanation you reviewed · The explanation without the errors' }),
    ).toBeInTheDocument();
    const reviewed = within(pane('The explanation you reviewed'));
    const planted = reviewed.getByText(PLANTED).closest('mark');
    expect(planted).not.toBeNull();
    expect(within(planted as HTMLElement).getByText('Planted error:', { exact: false })).toBeInTheDocument();
    expect(reviewed.getByText('Spacing helps recall.').closest('mark')).toBeNull();
    const clean = within(pane('The explanation without the errors'));
    expect(clean.getByText(CLEAN).closest('mark')).not.toBeNull();
    expect(clean.queryByText(PLANTED)).toBeNull();
  });

  it('shows the source passage under them, with the words that settle the issue marked in it', () => {
    setup();
    const source = within(pane('What the source says'));
    expect(source.getByText('Spacing and memory')).toBeInTheDocument();
    const quoted = source.getByText(QUOTE);
    expect(quoted.tagName).toBe('MARK');
    expect(pane('What the source says')).toHaveTextContent('The effect was smaller for what they already knew.');
  });

  it('shows the quote of the issue chosen in the source, and its sentences in both versions', async () => {
    setup();
    expect(within(pane('What the source says')).getByText(QUOTE).className).not.toContain('ring-accent');
    await userEvent.click(screen.getByRole('button', { name: 'Show Sentence 2 in the explanation' }));
    expect(pane('What the source says').querySelector('mark')?.className).toContain('ring-accent');
    expect(within(pane('The explanation you reviewed')).getByText(PLANTED).closest('mark')?.className).toContain('ring-accent');
    expect(within(pane('The explanation without the errors')).getByText(CLEAN).closest('mark')?.className).toContain('ring-accent');
  });

  it('says that the source passage decided the fix and shows no hidden suite for it', () => {
    setup();
    const fix = screen.getByRole('region', { name: 'Your fix' });
    expect(within(fix).getByText('Decided by the source passage')).toBeInTheDocument();
    expect(within(fix).getByText('The source supports what you wrote.')).toBeInTheDocument();
    expect(within(fix).queryByText('The hidden suite did not run.')).toBeNull();
  });

  it('shows the comment on its sentences', () => {
    setup();
    expect(screen.getByRole('article', { name: 'Comment 1' })).toHaveTextContent('Sentence 2');
  });

  it('shows no source and no quote for a review of code', () => {
    render(<DrillReveal result={gradedResult()} onNextDrill={vi.fn()} onRegrade={vi.fn()} regrading={false} />);
    expect(screen.queryByText('What the source says')).toBeNull();
    expect(screen.getByRole('heading', { level: 2, name: 'The code you reviewed · The code without the issues' })).toBeInTheDocument();
  });
});
