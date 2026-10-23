import { describe, expect, it } from 'vitest';

import { freshnessWords, itemVerdict, passMarkWords, percent, quizStatusWords, shownScore, submissionHeadline } from '@/features/learning/quizzes/quizCopy';
import { itemResult, quizForm, quizStatus, submission } from '@/features/learning/quizzes/quizTestData';

const attempt = (score: number, extra: Partial<{ formId: string; overriddenItems: number; pendingItems: number }> = {}) => ({
  formId: 'f', score, passed: score >= 0.7, gradeStatus: 'deterministic' as const, submittedAt: 0, retakeOfFormId: null, overriddenItems: 0, pendingItems: 0, attemptNumber: 1, ...extra,
});

describe('quiz status in words', () => {
  it('says every verdict in words', () => {
    expect(quizStatusWords(quizStatus()).label).toBe('Not taken');
    expect(quizStatusWords(quizStatus({ verdict: 'passed', best: attempt(0.85), attempts: 1 })).label).toBe(`Passed · ${percent(0.85)}`);
    expect(quizStatusWords(quizStatus({ verdict: 'not_passed', best: attempt(0.63), attempts: 2 })).label).toBe(`Not passed yet · best ${percent(0.63)}`);
    expect(quizStatusWords(quizStatus({ activeFormId: 'form', activeFormPaused: true }))).toMatchObject({ label: 'Paused · Resume', tone: 'paused' });
    expect(quizStatusWords(quizStatus({ activeFormId: 'form' })).label).toBe('In progress · Resume');
    expect(quizStatusWords(quizStatus({ bankStatus: 'not_prepared' })).label).toBe('Questions not prepared yet');
  });

  it('never reads a pending result as a fail', () => {
    const words = quizStatusWords(quizStatus({ verdict: 'pending', best: attempt(0.4, { pendingItems: 2 }), latest: attempt(0.4, { pendingItems: 2 }), attempts: 1 }));
    expect(words.label).toBe('Result pending · 2 written answers grading');
    expect(words.tone).toBe('pending');
    expect(words.label).not.toMatch(/not passed|fail/i);
  });

  it('labels a passing score that includes an override', () => {
    expect(quizStatusWords(quizStatus({ verdict: 'passed', best: attempt(0.85, { overriddenItems: 1 }), attempts: 1 })).label).toBe(`Passed · ${percent(0.85)}, includes your override`);
  });

  it('says how many questions a retake repeats', () => {
    expect(freshnessWords(quizStatus({ attempts: 1, formSize: 8, freshItems: 8 }))).toBe('A retake uses questions you have not seen.');
    expect(freshnessWords(quizStatus({ attempts: 2, formSize: 8, freshItems: 5 }))).toBe('A retake uses 5 fresh questions and repeats 3 questions you have seen, each labelled "Seen before".');
  });
});

describe('item verdicts', () => {
  it('words a keyed item by its points', () => {
    expect(itemVerdict(itemResult(), { active: false, stopped: false }).label).toBe('Correct');
    expect(itemVerdict(itemResult({ pointsEarned: 0 }), { active: false, stopped: false }).label).toBe('Not correct');
    expect(itemVerdict(itemResult({ pointsEarned: 2, pointsPossible: 3, detail: '2 of 3 pairs matched' }), { active: false, stopped: false })).toMatchObject({ label: 'Partly correct', detail: '2 of 3 pairs matched' });
  });

  it('a pending item is Grading… or Couldn’t grade yet, never Not correct', () => {
    const pending = itemResult({ gradeStatus: 'pending', pointsEarned: 0, pointsPossible: 4 });
    expect(itemVerdict(pending, { active: true, stopped: false })).toMatchObject({ label: 'Grading…', lostPoints: false, pending: true });
    expect(itemVerdict(pending, { active: false, stopped: false })).toMatchObject({ label: "Couldn't grade yet", tone: 'warning', lostPoints: false });
    expect(itemVerdict(pending, { active: false, stopped: true }).label).toBe('Grading stopped');
  });

  it('labels an override and counts it as full credit', () => {
    expect(itemVerdict(itemResult({ gradeStatus: 'provisional', learnerOverride: true, pointsEarned: 4, pointsPossible: 4 }), { active: false, stopped: false })).toMatchObject({ label: 'Overridden by you', lostPoints: false });
  });

  it('words model grades as provisional with their points', () => {
    expect(itemVerdict(itemResult({ gradeStatus: 'provisional', pointsEarned: 3, pointsPossible: 4 }), { active: false, stopped: false })).toMatchObject({ label: 'Provisional', detail: '3 of 4 points', lostPoints: true });
  });
});

describe('result headline', () => {
  it('gives no pass or fail while answers are grading, and shows only the keyed score', () => {
    const form = quizForm({ status: 'submitted', submission: submission({ passed: null, score: 0.4, pendingItems: 1, itemResults: [
      itemResult({ itemId: 'a', pointsEarned: 1, pointsPossible: 1 }), itemResult({ itemId: 'b', pointsEarned: 0, pointsPossible: 1 }),
      itemResult({ itemId: 'c', gradeStatus: 'pending', pointsEarned: 0, pointsPossible: 4 }),
    ] }) });
    expect(submissionHeadline(form)).toMatchObject({ label: 'Result pending', tone: 'pending' });
    expect(shownScore(form)?.line).toBe(`keyed answers ${percent(0.5)} · 1 of 2 points graded so far`);
  });

  it('says Passed or Not passed yet once every answer is graded', () => {
    expect(submissionHeadline(quizForm({ submission: submission({ passed: true }) })).label).toBe('Passed');
    expect(submissionHeadline(quizForm({ submission: submission({ passed: false }) })).label).toBe('Not passed yet');
  });

  it('says where the score sits against the pass mark, and nothing while grading', () => {
    expect(passMarkWords(quizForm({ submission: submission({ passed: true, score: 0.88 }) }))).toBe(`18 % above the ${percent(0.7)} pass mark.`);
    expect(passMarkWords(quizForm({ submission: submission({ passed: false, score: 0.63 }) }))).toBe(`7 % short of the ${percent(0.7)} pass mark. A retake uses fresh questions.`);
    expect(passMarkWords(quizForm({ submission: submission({ passed: null, score: 0.3 }) }))).toBeUndefined();
  });
});
