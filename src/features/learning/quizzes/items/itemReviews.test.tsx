import { render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { blankStates, normaliseAnswer } from '@/features/learning/quizzes/items/answerMatch';
import { itemRegistry } from '@/features/learning/quizzes/items/itemRegistry';
import { draftOf } from '@/features/learning/quizzes/items/itemResponse';
import { formItem, itemResult, reveal } from '@/features/learning/quizzes/quizTestData';
import type { LearningAssessmentFormItemDto, LearningAssessmentItemResultDto, LearningKeyRevealDto } from '@/lib/bindings';

vi.mock('@/components/TiptapEditor', () => ({ TiptapViewer: ({ content }: { content: string }) => <div>{content}</div> }));

function review(item: LearningAssessmentFormItemDto, key: LearningKeyRevealDto, result: LearningAssessmentItemResultDto | null = null) {
  const { Review } = itemRegistry[item.format];
  return render(<Review item={item} draft={draftOf(item)} reveal={key} result={result} />);
}

describe('every wrong part says why, in words', () => {
  it('single choice names your pick and the key, and gives your pick’s flaw first', () => {
    review(formItem('single_choice', { options: ['Km rises', 'Vmax falls', 'Both fall', 'Nothing changes'], selectedIndices: [1] }),
      reveal({ correctIndices: [0], explanation: 'A competitive inhibitor raises the apparent Km.', distractorRationales: ['', 'Vmax is unchanged at saturation.', 'Only uncompetitive lowers both.', 'The curve shifts.'] }));
    expect(screen.getByText('You chose B; the key is A. Vmax is unchanged at saturation.')).toBeVisible();
    expect(screen.getByText('Your answer · not correct')).toBeVisible();
    expect(screen.getByText('Correct answer')).toBeVisible();
    expect(screen.getByText('Why the others fail')).toBeVisible();
    expect(screen.getByText('Only uncompetitive lowers both.')).toBeInTheDocument();
  });

  it('multi-select says each pick and miss', () => {
    review(formItem('multi_select', { options: ['a', 'b', 'c', 'd'], selectedIndices: [0, 1] }),
      reveal({ correctIndices: [0, 2], distractorRationales: ['', 'b is a decoy because…', '', ''] }), itemResult({ detail: '1 of 2 right, 1 wrong pick', pointsEarned: 0, pointsPossible: 1 }));
    expect(screen.getByText('You picked it · in the key')).toBeVisible();
    expect(screen.getByText('You picked it · not in the key')).toBeVisible();
    expect(screen.getByText('Missed · in the key')).toBeVisible();
    expect(screen.getByText('b is a decoy because…')).toBeVisible();
  });

  it('matching says each pair right or wrong and what it matches', () => {
    review(formItem('matching', { leftItems: ['ATP', 'Enzyme'], rightItems: ['Catalyst', 'Energy currency'], matches: [1, 1] }), reveal({ matches: [1, 0] }));
    const pairs = screen.getAllByRole('listitem');
    expect(within(pairs[0]).getByText('Correct')).toBeVisible();
    expect(within(pairs[1]).getByText('Not correct')).toBeVisible();
    expect(pairs[1]).toHaveTextContent('Enzyme matches Catalyst, not Energy currency.');
  });

  it('ordering shows your order beside the correct one, each position in words', () => {
    review(formItem('ordering', { options: ['Bind', 'Convert', 'Release'], orderedValues: ['Convert', 'Bind', 'Release'] }), reveal({ orderedValues: ['Bind', 'Convert', 'Release'] }));
    const yours = within(screen.getByRole('region', { name: 'Your order' }));
    expect(yours.getByText('Belongs at 2')).toBeVisible();
    expect(yours.getByText('Belongs at 1')).toBeVisible();
    expect(yours.getByText('In place')).toBeVisible();
  });

  it('short answer shows your words beside the accepted answer', () => {
    review(formItem('short_answer', { textResponse: '5' }), reveal({ acceptedAnswer: '7' }), itemResult({ pointsEarned: 0, pointsPossible: 1, detail: 'You gave 5; the answer is 7 ± 0.5' }));
    expect(screen.getByText('Not accepted')).toBeVisible();
    expect(screen.getByText('“5”')).toBeVisible();
    expect(screen.getByText('7')).toBeVisible();
  });
});

describe('cloze blanks', () => {
  it('normalises the way the grader does', () => {
    expect(normaliseAnswer('  The Café, ')).toBe('cafe');
    expect(normaliseAnswer('1,000 mM')).toBe('1000 mm');
    expect(normaliseAnswer('3.2')).toBe('3.2');
  });

  it('never calls a blank wrong when another accepted spelling might have matched', () => {
    // One of two blanks earned a point, but neither matches the shown key: which one is unknown.
    expect(blankStates(['colour', 'enzym'], ['color', 'enzyme'], 0.5)).toEqual(['unknown', 'unknown']);
    // The matched blank accounts for the earned share, so the other is wrong.
    expect(blankStates(['color', 'enzym'], ['color', 'enzyme'], 0.5)).toEqual(['right', 'wrong']);
    expect(blankStates(['colour', 'x'], ['color', 'enzyme'], 1)).toEqual(['right', 'right']);
  });

  it('reviews each blank with its verdict', () => {
    review(formItem('cloze', { prompt: 'The {{1}} binds the {{2}}.', blankCount: 2, blankResponses: ['inhibitor', 'substrate'] }),
      reveal({ blanks: ['inhibitor', 'enzyme'] }), itemResult({ pointsEarned: 0.5, pointsPossible: 1, gradeStatus: 'deterministic' }));
    expect(screen.getByText('Correct')).toBeVisible();
    expect(screen.getByText('Not correct')).toBeVisible();
    expect(screen.getByText(/accepted:/)).toHaveTextContent('· accepted: enzyme');
  });
});
