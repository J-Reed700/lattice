import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { QuizResult, type LessonCompletion } from '@/features/learning/quizzes/QuizResult';
import { criterion, formItem, itemResult, quizForm, rationale, reveal, submission } from '@/features/learning/quizzes/quizTestData';
import type { LearningAssessmentFormDto, LearningGradeContestDto } from '@/lib/bindings';

vi.mock('@/components/TiptapEditor', () => ({ TiptapViewer: ({ content }: { content: string }) => <div>{content}</div> }));

const api = vi.hoisted(() => ({ resume: vi.fn(), cancel: vi.fn(), contest: vi.fn(), override: vi.fn(), report: vi.fn(), misses: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  resumeLearningQuizGrading: api.resume, cancelLearningGenerationJob: api.cancel, contestLearningQuizGrade: api.contest,
  overrideLearningQuizGrade: api.override, reportLearningQuizItem: api.report, addLearningQuizMissesToRecall: api.misses,
} }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const choice = formItem('single_choice', { id: 'choice', prompt: 'Which step is rate-limiting?', options: ['Binding', 'Release'], selectedIndices: [1], keyReveal: reveal({ correctIndices: [0] }) });
const written = formItem('explanation', { id: 'written', prompt: 'Explain uncompetitive inhibition.', points: 4, textResponse: 'The inhibitor binds the ES complex.', keyReveal: reveal() });

function show(form: LearningAssessmentFormDto, completion?: LessonCompletion) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return render(<QuizResult form={form} sources={[]} programRevision={3} onBack={vi.fn()} onRetake={vi.fn()} retakePending={false} completion={completion} />, { wrapper });
}

const submitted = (overrides: Partial<LearningAssessmentFormDto['submission'] & object>) => quizForm({ status: 'submitted', submittedAt: Date.UTC(2026, 9, 23), items: [choice, written], submission: submission(overrides) });

describe('quiz result', () => {
  beforeEach(() => vi.resetAllMocks());

  it('never reads as a fail while a written answer is grading', () => {
    show(submitted({ passed: null, score: 0, pendingItems: 1, gradingJobId: 'job-1', itemResults: [
      itemResult({ itemId: 'choice', pointsEarned: 1, pointsPossible: 1 }),
      itemResult({ itemId: 'written', gradeStatus: 'pending', pointsEarned: 0, pointsPossible: 4, correct: null }),
    ] }));
    expect(screen.getByRole('heading', { level: 2 })).toHaveTextContent('Result pending');
    expect(screen.getByRole('heading', { level: 2 })).toHaveFocus();
    expect(screen.getByText(/keyed answers 100/)).toBeVisible();
    expect(screen.getByText('Grading…')).toBeVisible();
    expect(screen.queryByText(/Not passed/)).not.toBeInTheDocument();
    expect(screen.queryByText('Not correct')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Stop/ })).toBeVisible();
    expect(screen.queryByRole('button', { name: /Add .* to Recall/ })).not.toBeInTheDocument();
  });

  it("says Couldn't grade yet with Retry grading when the grader stopped short", async () => {
    const user = userEvent.setup();
    const form = submitted({ passed: null, pendingItems: 1, gradingJobId: null, itemResults: [
      itemResult({ itemId: 'choice' }),
      itemResult({ itemId: 'written', gradeStatus: 'pending', pointsEarned: 0, pointsPossible: 4, feedback: "The model didn't explain this grade." }),
    ] });
    api.resume.mockResolvedValue(ok(form));
    show(form);
    expect(screen.getByText("Couldn't grade yet")).toBeVisible();
    expect(screen.getByText("The model didn't explain this grade.")).toBeVisible();
    expect(screen.queryByText('Not correct')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Retry grading' }));
    await waitFor(() => expect(api.resume).toHaveBeenCalledWith(expect.objectContaining({ programId: 'program-1', formId: 'form-1', operationId: expect.any(String) })));
  });

  it('offers Mark lesson complete after a pass and never completes the lesson by itself', async () => {
    const user = userEvent.setup();
    const onComplete = vi.fn();
    const form = submitted({ passed: true, score: 0.85, itemResults: [itemResult({ itemId: 'choice' }), itemResult({ itemId: 'written', gradeStatus: 'provisional', pointsEarned: 3, pointsPossible: 4, rationale: rationale() })], graderModel: 'Qwen3.5-9B' });
    const { rerender } = show(form, { completed: false, pending: false, onComplete });
    expect(screen.getByRole('heading', { level: 2 })).toHaveTextContent('Passed');
    expect(screen.getByText(/Written answers: provisional, graded by Qwen3\.5-9B/)).toBeVisible();
    expect(onComplete).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: 'Mark lesson complete' }));
    expect(onComplete).toHaveBeenCalledTimes(1);
    rerender(<QuizResult form={form} sources={[]} programRevision={3} onBack={vi.fn()} onRetake={vi.fn()} retakePending={false} completion={{ completed: true, pending: false, onComplete }} />);
    expect(screen.getByText('Lesson marked complete')).toBeVisible();
  });

  it('explains a lost point and adds the missed question to Recall', async () => {
    const user = userEvent.setup();
    api.misses.mockResolvedValue(ok({ cardsCreated: 2, deckId: 'deck' }));
    show(submitted({ passed: false, score: 0.4, itemResults: [
      itemResult({ itemId: 'choice', pointsEarned: 0 }),
      itemResult({ itemId: 'written', gradeStatus: 'provisional', pointsEarned: 2, pointsPossible: 4, rationale: rationale({ criteria: [criterion({ verdict: 'missed', points: 0, whatWasMissing: 'No example given.' })] }) }),
    ] }));
    expect(screen.getByRole('heading', { level: 2 })).toHaveTextContent('Not passed yet');
    expect(screen.getByText('Not correct')).toBeVisible();
    expect(screen.getByText('Why this lost points').closest('details')).toHaveAttribute('open');
    await user.click(screen.getByRole('button', { name: 'Add 2 missed questions to Recall' }));
    await waitFor(() => expect(api.misses).toHaveBeenCalledWith(expect.objectContaining({ itemIds: ['choice', 'written'] })));
    expect(await screen.findByText('Added 2 cards to Recall. You can edit them there.')).toBeVisible();
  });

  it('allows an override only after an upheld regrade, and labels it everywhere', async () => {
    const user = userEvent.setup();
    const contest: LearningGradeContestDto = { id: 'c', status: 'upheld', note: 'I gave an example.', firstScore: 2, regradeScore: 2, regradeReason: null,
      firstRationale: rationale(), regradeRationale: rationale(), responseToNote: 'The example named no inhibitor.', createdAt: 0, decidedAt: 1 };
    const before = submitted({ passed: false, score: 0.6, itemResults: [itemResult({ itemId: 'choice' }), itemResult({ itemId: 'written', gradeStatus: 'provisional', pointsEarned: 2, pointsPossible: 4, rationale: rationale(), contest })] });
    const after = submitted({ passed: true, score: 1, scoreWithoutOverrides: 0.6, itemResults: [itemResult({ itemId: 'choice' }), itemResult({ itemId: 'written', gradeStatus: 'provisional', pointsEarned: 4, pointsPossible: 4, learnerOverride: true, rationale: rationale(), contest: { ...contest, status: 'overridden' } })] });
    api.override.mockResolvedValue(ok(after));
    const { rerender } = show(before);
    const regrade = within(screen.getByRole('region', { name: 'Your regrade request' }));
    expect(regrade.getByText('Regrade upheld the grade')).toBeVisible();
    expect(regrade.getByText('The example named no inhibitor.')).toBeVisible();
    expect(regrade.getByRole('region', { name: 'First grade' })).toBeVisible();
    await user.click(regrade.getByRole('button', { name: 'Override: count it as right' }));
    await user.click(regrade.getByRole('button', { name: 'Override' }));
    await waitFor(() => expect(api.override).toHaveBeenCalledWith(expect.objectContaining({ itemId: 'written', formId: 'form-1' })));
    rerender(<QuizResult form={after} sources={[]} programRevision={3} onBack={vi.fn()} onRetake={vi.fn()} retakePending={false} />);
    expect(screen.getAllByText('Overridden by you').length).toBeGreaterThan(0);
    expect(screen.getByText(/Score includes your override \(1 answer overridden by you\)/)).toBeVisible();
  });
});
