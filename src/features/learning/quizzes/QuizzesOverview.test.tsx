import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { LessonQuizCallout } from '@/features/learning/quizzes/LessonQuizCallout';
import { percent } from '@/features/learning/quizzes/quizCopy';
import { quizForm, quizOverview, quizStatus } from '@/features/learning/quizzes/quizTestData';
import { QuizzesOverview } from '@/features/learning/quizzes/QuizzesOverview';
import type { LearningLessonDto, LearningProgramDto, LearningQuizOverviewDto } from '@/lib/bindings';

vi.mock('@/components/TiptapEditor', () => ({ TiptapViewer: ({ content }: { content: string }) => <div>{content}</div> }));

const api = vi.hoisted(() => ({ overview: vi.fn(), start: vi.fn(), prepare: vi.fn(), plan: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningQuizOverview: api.overview, startLearningQuiz: api.start, prepareLearningQuizBank: api.prepare, getLearningPlan: api.plan,
} }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const lesson = (id: string, title: string, ready: boolean) => ({ id, title, objective: '', estimatedMinutes: 20, preparation: ready ? 'ready' : 'outline', blocks: [], completed: false }) as unknown as LearningLessonDto;
const program = {
  summary: { id: 'program-1', status: 'active', revision: 1 }, sources: [],
  modules: [{ id: 'module-1', title: 'Foundations', summary: '', outcomes: [], lessons: [lesson('lesson-1', 'Enzyme kinetics', true), lesson('lesson-2', 'Inhibition', false)] }],
} as unknown as LearningProgramDto;
const attempt = (score: number) => ({ formId: 'best', score, passed: score >= 0.7, gradeStatus: 'deterministic' as const, submittedAt: 0, retakeOfFormId: null, overriddenItems: 0, pendingItems: 0 });

function wrap(ui: ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
}

function withLesson(quiz: Partial<LearningQuizOverviewDto['modules'][number]['lessons'][number]['quiz']>) {
  const base = quizOverview();
  base.modules[0].lessons[0].quiz = quizStatus(quiz);
  return base;
}

describe('Quizzes overview', () => {
  beforeEach(() => vi.resetAllMocks());

  it('lists the exam, each module test and every lesson quiz with its verdict in words', async () => {
    const overview = withLesson({ verdict: 'not_passed', best: attempt(0.63), latest: attempt(0.63), attempts: 2, freshItems: 5 });
    overview.quizzesPassed = 0;
    api.overview.mockResolvedValue(ok(overview));
    wrap(<QuizzesOverview program={program} formId={null} onFormIdChange={vi.fn()} onOpenLesson={vi.fn()} />);
    expect(await screen.findByRole('heading', { name: 'Check what stuck, whenever you like' })).toBeVisible();
    expect(screen.getByText(`Pass marks: quiz ${percent(0.7)} · test ${percent(0.75)} · exam ${percent(0.75)}`)).toBeVisible();
    expect(screen.getByRole('heading', { name: 'Course exam' })).toBeVisible();
    const module = within(screen.getByRole('region', { name: /Module 1/ }));
    expect(module.getByText(`Not passed yet · best ${percent(0.63)}`)).toBeVisible();
    expect(module.getByText(/repeats 3 questions you have seen/)).toBeVisible();
    // An unprepared lesson is not locked: it says how its quiz gets written.
    expect(module.getByRole('button', { name: 'Prepare the lesson' })).toBeVisible();
    expect(module.getAllByText('Covers 1 of 2 lessons in this module', { exact: false }).length).toBeGreaterThan(0);
    expect(screen.getByRole('region', { name: 'Four ways to observe learning' })).toBeVisible();
  });

  it('starts a retake with fresh questions from the latest attempt', async () => {
    const user = userEvent.setup();
    api.overview.mockResolvedValue(ok(withLesson({ verdict: 'not_passed', best: attempt(0.63), latest: { ...attempt(0.5), formId: 'latest' }, attempts: 2 })));
    api.start.mockResolvedValue(ok(quizForm({ id: 'new-form' })));
    const onFormIdChange = vi.fn();
    wrap(<QuizzesOverview program={program} formId={null} onFormIdChange={onFormIdChange} onOpenLesson={vi.fn()} />);
    const module = within(await screen.findByRole('region', { name: /Module 1/ }));
    await user.click(module.getByRole('button', { name: 'Retake' }));
    await waitFor(() => expect(onFormIdChange).toHaveBeenCalledWith('new-form'));
    expect(api.start).toHaveBeenCalledWith(expect.objectContaining({ scope: { kind: 'lesson', id: 'lesson-1' }, mode: 'graded', retakeOfFormId: 'latest' }));
  });
});

describe('lesson quiz callout', () => {
  beforeEach(() => vi.resetAllMocks());
  const enzyme = program.modules[0].lessons[0];

  it('offers the quiz, a practice round, and says nothing has been taken', async () => {
    api.overview.mockResolvedValue(ok(quizOverview()));
    wrap(<LessonQuizCallout program={program} lesson={enzyme} onOpenForm={vi.fn()} onViewResult={vi.fn()} />);
    expect(await screen.findByRole('button', { name: /Start quiz/ })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Practice first' })).toBeVisible();
    expect(screen.getByText('Best: not taken yet')).toBeVisible();
    expect(screen.getByText('8 questions · about 12 min')).toBeVisible();
  });

  it('says a pass in words and offers a retake', async () => {
    api.overview.mockResolvedValue(ok(withLesson({ verdict: 'passed', best: attempt(0.88), latest: attempt(0.88), attempts: 1 })));
    wrap(<LessonQuizCallout program={program} lesson={enzyme} onOpenForm={vi.fn()} onViewResult={vi.fn()} />);
    expect(await screen.findByText(`Passed · ${percent(0.88)}`)).toBeVisible();
    expect(screen.getByRole('button', { name: /Retake/ })).toBeVisible();
    expect(screen.getByRole('button', { name: 'Review' })).toBeVisible();
  });

  it('shows the bank job’s step while questions are being prepared', async () => {
    api.overview.mockResolvedValue(ok(withLesson({ bankStatus: 'preparing', preparingJobId: 'job-1' })));
    api.plan.mockResolvedValue(ok({ programId: 'program-1', jobs: [{ id: 'job-1', status: 'running', progressCompleted: 1, progressTotal: 3, progressMessage: 'writing short answers' }] }));
    wrap(<LessonQuizCallout program={program} lesson={enzyme} onOpenForm={vi.fn()} onViewResult={vi.fn()} />);
    expect(await screen.findByText('Preparing questions… step 2 of 3 (writing short answers)')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Preparing…' })).toBeDisabled();
  });

  it('resumes a paused quiz by its form', async () => {
    const user = userEvent.setup();
    const onOpenForm = vi.fn();
    api.overview.mockResolvedValue(ok(withLesson({ activeFormId: 'paused-form' })));
    wrap(<LessonQuizCallout program={program} lesson={enzyme} onOpenForm={onOpenForm} onViewResult={vi.fn()} />);
    await user.click(await screen.findByRole('button', { name: 'Resume' }));
    expect(onOpenForm).toHaveBeenCalledWith('paused-form');
    expect(screen.getByText('Paused · resume or start fresh')).toBeVisible();
  });
});
