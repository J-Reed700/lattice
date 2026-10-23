import { render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { itemRegistry } from '@/features/learning/quizzes/items/itemRegistry';
import { draftOf } from '@/features/learning/quizzes/items/itemResponse';
import { answerHintWords } from '@/features/learning/quizzes/items/ShortAnswerItem';
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

describe('graded parts', () => {
  it('cloze says each blank right or wrong from the grader, with its reason', () => {
    review(formItem('cloze', { prompt: 'The {{1}} binds the {{2}}.', blankCount: 2, blankResponses: ['inhibitor', 'substrate'] }),
      reveal({ blanks: ['inhibitor', 'enzyme'] }),
      itemResult({ pointsEarned: 0.5, pointsPossible: 1, partResults: [{ index: 0, correct: true, detail: '' }, { index: 1, correct: false, detail: 'The inhibitor binds the enzyme, not the substrate.' }] }));
    expect(screen.getByText('Correct')).toBeVisible();
    expect(screen.getByText('Not correct')).toBeVisible();
    expect(screen.getByText('The inhibitor binds the enzyme, not the substrate.')).toBeVisible();
    expect(screen.getByText(/accepted:/)).toHaveTextContent('· accepted: enzyme');
  });

  it('cloze gives no verdict before grading, only the accepted answer', () => {
    review(formItem('cloze', { prompt: 'The {{1}} binds.', blankCount: 1, blankResponses: ['colour'] }), reveal({ blanks: ['color'] }));
    expect(screen.queryByText('Correct')).not.toBeInTheDocument();
    expect(screen.queryByText('Not correct')).not.toBeInTheDocument();
    expect(screen.getByText(/accepted:/)).toHaveTextContent('· accepted: color');
  });

  it('matching and ordering name the wrong part and say why', () => {
    review(formItem('matching', { leftItems: ['ATP', 'Enzyme'], rightItems: ['Catalyst', 'Energy currency'], matches: [1, 1] }), reveal({ matches: [1, 0] }),
      itemResult({ pointsEarned: 0.5, partResults: [{ index: 0, correct: true, detail: '' }, { index: 1, correct: false, detail: 'An enzyme speeds a reaction; it is not spent.' }] }));
    expect(screen.getByText('An enzyme speeds a reaction; it is not spent.')).toBeVisible();
  });

  it('cloze prompts send maths through the viewer so it renders', () => {
    review(formItem('cloze', { prompt: 'When $[S] = K_m$, the rate is {{1}} of Vmax.', blankCount: 1 }), reveal({ blanks: ['half'] }));
    expect(screen.getByText('When $[S] = K_m$, the rate is')).toBeInTheDocument();
  });
});

describe('short answer hints', () => {
  it('says a number and its unit before submission, never the key', () => {
    expect(answerHintWords({ numeric: true, unitHint: 'mM' })).toBe('A number, in mM. Writing the unit is optional.');
    expect(answerHintWords({ numeric: true, unitHint: null })).toBe('A number.');
    expect(answerHintWords(null)).toMatch(/^A word or a short phrase/);
  });
});
