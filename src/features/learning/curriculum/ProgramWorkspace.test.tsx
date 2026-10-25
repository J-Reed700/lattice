import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ProgramWorkspace } from '@/features/learning/curriculum/ProgramWorkspace';
import { review, unsupportedItem } from '@/features/learning/lessons/review/claimReviewTestData';
import { quizOverview } from '@/features/learning/quizzes/quizTestData';
import type { LearningGenerationJob, LearningLessonDto, LearningProgramDto } from '@/lib/bindings';

vi.mock('@/features/learning/tangents/ProgramTangents', () => ({
  ProgramTangents: ({ children }: { children: ReactNode }) => children,
  ProgramTangentsTrigger: () => null,
}));
vi.mock('@/features/learning/tangents/StudyTangentSelection', () => ({
  StudyTangentSelection: ({ children }: { children: ReactNode }) => children,
  SectionTangentBar: () => null,
}));
vi.mock('@/features/learning/curriculum/CourseCoach', () => ({ CourseCoach: () => null }));
vi.mock('@/features/learning/curriculum/CourseSyllabus', () => ({ CourseSyllabus: () => null }));
vi.mock('@/features/reading/components/CitationVisibilityToggle', () => ({ CitationVisibilityToggle: () => null }));

const api = vi.hoisted(() => ({ plan: vi.fn(), review: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningPlan: api.plan,
  getLearningLessonClaimReview: api.review,
  getLearningLessonEvidence: async () => ({ ok: true, data: null }),
  getLearningOutlineEvidence: async () => ({ ok: true, data: null }),
  getLearningMemory: async () => ({ ok: true, data: { dueCount: 0 } }),
  getLearningQuizOverview: async () => ({ ok: true, data: quizOverview({ quizzesPassed: 1 }) }),
  getLearningRecallWorkspace: async () => ({ ok: true, data: { programId: 'program-1', cards: [], duplicates: [], schedulerDisclosure: '' } }),
  getLearningSourceWorkspace: async () => ({ ok: true, data: { programId: 'program-1', sources: [] } }),
  courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
  courseIndexOpen: async () => ({ ok: true, data: undefined }),
  courseIndexClose: async () => ({ ok: true, data: undefined }),
} }));

// A course whose title is its whole goal, as an outline generated from a long goal can be.
const GOAL = 'Understand how the circulatory system moves oxygen from the lungs to every cell, and why it fails in heart disease.';

const held = { id: 'lesson-1', title: 'The heart as a pump', objective: 'Explain the cardiac cycle.', estimatedMinutes: 20, preparation: 'outline', blocks: [], completed: false } as unknown as LearningLessonDto;
const next: LearningLessonDto = { ...held, id: 'lesson-2', title: 'Blood vessels' };

function program(title: string, goal = GOAL): LearningProgramDto {
  return {
    outlineReview: null,
    summary: { id: 'program-1', title, goal, status: 'active', revision: 9, currentLessonId: 'lesson-1' },
    priorKnowledge: '', minutesPerSession: 30, modelName: 'Test model',
    modules: [{ id: 'module-1', title: 'Circulation', summary: 'How blood moves.', outcomes: ['Describe the cardiac cycle'], lessons: [held, next] }],
    sources: [],
  } as unknown as LearningProgramDto;
}

const job = (overrides: Partial<LearningGenerationJob>): LearningGenerationJob => ({
  id: 'job-1', programId: 'program-1', operationId: 'op', kind: 'lesson_preparation', payloadSha256: 'hash', baseRevisionNumber: 9,
  status: 'running', progressCompleted: 0, progressTotal: 1, progressMessage: 'Writing the draft', activity: null,
  resultId: null, error: null, retryOfJobId: null, createdAt: Date.now() - 60_000, startedAt: Date.now() - 60_000, finishedAt: null,
  ...overrides,
});

function show(course: LearningProgramDto, onReview = vi.fn()) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={client}>
    <ProgramWorkspace program={course} onBack={vi.fn()} onOpenProgram={vi.fn()} onAccept={vi.fn()} acceptPending={false}
      onPrepare={vi.fn()} preparePending={false} onComplete={vi.fn()} completePending={false}
      onReview={onReview} />
  </QueryClientProvider>);
  return onReview;
}

describe('Course page', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    // One lesson held for review by a finished attempt, another being prepared now.
    api.plan.mockResolvedValue({ ok: true, data: { programId: 'program-1', jobs: [
      job({ id: 'job-2', createdAt: Date.now() }),
      job({ status: 'completed', resultId: 'needs_review:lesson-1', finishedAt: Date.now() - 120_000, createdAt: Date.now() - 600_000 }),
    ] } });
    api.review.mockImplementation(async (_programId: string, lessonId: string) => ({ ok: true, data: lessonId === 'lesson-1'
      ? review([unsupportedItem(1), unsupportedItem(2)], { lessonId })
      : null }));
  });

  it('the Lessons tab renders the goal once, one preparation status, and no inline claim review', async () => {
    show(program(GOAL));
    expect(await screen.findByText('2 claims need you')).toBeVisible();
    expect(screen.getAllByRole('region', { name: 'Lesson preparation' })).toHaveLength(1);
    expect(screen.queryAllByRole('region', { name: /claim review/i })).toHaveLength(0);
    // The title is the goal: shown once, with no Goal disclosure repeating it.
    expect(screen.getAllByText(GOAL)).toHaveLength(1);
    expect(screen.queryByText('Goal')).not.toBeInTheDocument();
    // No internal revision number and no claim text on the course page.
    expect(screen.queryByText(/Revision 9/)).not.toBeInTheDocument();
    expect(screen.queryByText('Statement number 1.')).not.toBeInTheDocument();
  });

  it('has Quizzes as a primary tab, and Practice keeps only written practice and code', async () => {
    show(program(GOAL));
    const tabs = within(screen.getByRole('tablist', { name: 'Program workspace' })).getAllByRole('tab');
    expect(tabs.map((tab) => tab.textContent)).toEqual(['Lessons', 'Practice', 'Quizzes', 'Recall', 'Notebook', 'Sources']);
    await userEvent.click(tabs[1]);
    const practice = await screen.findByRole('tablist', { name: 'Practice activities' });
    expect(within(practice).getAllByRole('tab').map((tab) => tab.textContent)).toEqual(['Written practice', 'Code & simulations']);
    expect(screen.queryByText(/Quick checks/)).not.toBeInTheDocument();
  });

  it('shows quizzes passed beside lessons complete in the course header', async () => {
    show(program(GOAL));
    expect(await screen.findByText('Quizzes passed')).toBeVisible();
  });

  it('keeps a goal that differs from the title behind one Goal disclosure', async () => {
    show(program('Circulation'));
    expect(await screen.findByRole('heading', { level: 1, name: 'Circulation' })).toBeVisible();
    expect(screen.getByText('Goal')).toBeVisible();
    expect(screen.getAllByText(GOAL)).toHaveLength(1);
  });

  it('renders StudyHabitsBar and opens the learning guide', async () => {
    const user = userEvent.setup();
    show(program(GOAL));
    const guideButton = await screen.findByRole('button', { name: 'How to learn so it lasts' });
    expect(guideButton).toBeVisible();
    await user.click(guideButton);
    expect(await screen.findByRole('dialog')).toBeVisible();
    expect(screen.getByRole('heading', { name: 'How to learn so it lasts' })).toBeVisible();
  });

  it('allows switching written practice to Feynman technique (Teach a beginner)', async () => {
    const user = userEvent.setup();
    show(program(GOAL));
    const tabs = within(screen.getByRole('tablist', { name: 'Program workspace' })).getAllByRole('tab');
    await user.click(tabs[1]);
    const practiceModes = await screen.findByRole('group', { name: 'Written practice mode' });
    expect(practiceModes).toBeVisible();
    const feynmanButton = within(practiceModes).getByRole('button', { name: 'Feynman technique (Teach a beginner)' });
    await user.click(feynmanButton);
    expect(feynmanButton).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByText(/The Feynman Technique · Teach a Beginner/i)).toBeVisible();
  });

  it('allows switching recall to Free recall (Brain dump) and Versioned cards & schedules', async () => {
    const user = userEvent.setup();
    show(program(GOAL));
    const tabs = within(screen.getByRole('tablist', { name: 'Program workspace' })).getAllByRole('tab');
    await user.click(tabs[3]);
    const recallModes = await screen.findByRole('group', { name: 'Recall view' });
    expect(recallModes).toBeVisible();

    const brainDumpButton = within(recallModes).getByRole('button', { name: 'Free recall (Brain dump)' });
    await user.click(brainDumpButton);
    expect(brainDumpButton).toHaveAttribute('aria-pressed', 'true');
    expect(await screen.findByText(/Close your books & write everything from memory/i)).toBeVisible();

    const evolutionButton = within(recallModes).getByRole('button', { name: 'Versioned cards & schedules' });
    await user.click(evolutionButton);
    expect(evolutionButton).toHaveAttribute('aria-pressed', 'true');
    expect(await screen.findByTestId('learning-recall-evolution')).toBeVisible();
  });
});
