import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { GradeRationale } from '@/features/learning/quizzes/GradeRationale';
import { criterion, rationale } from '@/features/learning/quizzes/quizTestData';
import { listNumbers, numberQuotes } from '@/features/learning/quizzes/rationaleHighlights';

const ANSWER = 'The inhibitor binds the ES complex, so both Vmax and Km fall. It cannot be outcompeted.';

describe('numbering the learner’s own words', () => {
  it('numbers quotes in reading order and ties each number to its criteria', () => {
    const criteria = [
      { yourWords: ['It cannot be outcompeted'] },
      { yourWords: ['binds the ES complex', 'It cannot be outcompeted'] },
      { yourWords: [] },
    ];
    const { segments, byCriterion, loose } = numberQuotes(ANSWER, criteria);
    expect(segments.filter((segment) => segment.quote).map((segment) => [segment.quote, segment.text])).toEqual([
      [1, 'binds the ES complex'],
      [2, 'It cannot be outcompeted'],
    ]);
    // A quote two criteria share keeps one number; an omission rests on none.
    expect(byCriterion).toEqual([[2], [1, 2], []]);
    expect(loose).toEqual([]);
    expect(segments.map((segment) => segment.text).join('')).toBe(ANSWER);
  });

  it('matches across line breaks the way the grader checks, and lists quotes it cannot place', () => {
    const { segments, loose } = numberQuotes('binds the\nES complex', [{ yourWords: ['binds the ES complex', 'never said this'] }]);
    expect(segments).toEqual([{ text: 'binds the\nES complex', quote: 1 }]);
    expect(loose).toEqual([{ quote: 2, text: 'never said this' }]);
  });

  it('lists numbers in words', () => {
    expect(listNumbers([1])).toBe('1');
    expect(listNumbers([1, 3])).toBe('1 and 3');
    expect(listNumbers([1, 2, 4])).toBe('1, 2 and 4');
  });
});

describe('Why this lost points', () => {
  const lost = rationale({
    summary: 'You explained how the inhibitor binds, but gave no concrete example, which the question asked for.',
    criteria: [
      criterion({ criterionId: 'accuracy', title: 'Accuracy', verdict: 'met', points: 2, maxPoints: 2, yourWords: ['inhibitor binds the ES complex'] }),
      criterion({ criterionId: 'example', title: 'Example', verdict: 'missed', points: 0, maxPoints: 2, whatWasMissing: 'No example given.', fullCreditIncludes: 'Names a real inhibitor and its effect.' }),
    ],
  });

  it('opens by default when points were lost, with the summary, numbered highlights, criteria and grader', () => {
    render(<GradeRationale rationale={lost} answer={ANSWER} lostPoints statusWord="provisional" sources={[]} />);
    const panel = screen.getByText('Why this lost points').closest('details')!;
    expect(panel).toHaveAttribute('open');
    expect(screen.getByText(/gave no concrete example/)).toBeVisible();
    const mark = panel.querySelector('mark[data-quote="1"]')!;
    expect(mark).toHaveTextContent('inhibitor binds the ES complex1 (quote 1)');
    const accuracy = within(screen.getByText('Accuracy').closest('li')!);
    expect(accuracy.getByText('Met')).toBeVisible();
    expect(accuracy.getByText('Rests on quote 1, in your words.')).toBeVisible();
    const example = within(screen.getByText('Example').closest('li')!);
    expect(example.getByText('Missed')).toBeVisible();
    expect(example.getByText('No example given.')).toBeVisible();
    expect(example.getByText('Names a real inhibitor and its effect.')).toBeVisible();
    expect(screen.getByText(/^Graded by Qwen3\.5-9B · provisional · /)).toBeVisible();
  });

  it('starts collapsed for a full-credit grade, behind a native disclosure', () => {
    render(<GradeRationale rationale={rationale()} answer={ANSWER} lostPoints={false} statusWord="provisional" sources={[]} />);
    const summary = screen.getByText('How this was graded').closest('summary')!;
    expect(summary.closest('details')).not.toHaveAttribute('open');
    expect(screen.queryByText('Why this lost points')).not.toBeInTheDocument();
  });
});
