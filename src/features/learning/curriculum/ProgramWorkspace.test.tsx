import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ProgramWorkspace } from '@/features/learning/curriculum/ProgramWorkspace';
import { review, unsupportedItem } from '@/features/learning/lessons/review/claimReviewTestData';
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
  getLearningMemory: async () => ({ ok: true, data: { programId: 'program-1', journalId: null, lessonNotes: [], studyDeck: null, drafts: [], acceptedCards: [], dueCount: 0 } }),
  courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
  courseIndexOpen: async () => ({ ok: true, data: undefined }),
  courseIndexClose: async () => ({ ok: true, data: undefined }),
} }));

// A course whose title is its whole goal, as an outline generated from a long goal can be.
const GOAL = 'Understand how the circulatory system moves oxygen from the lungs to every cell, and why it fails in heart disease.';

const held: LearningLessonDto = { id: 'lesson-1', title: 'The heart as a pump', objective: 'Explain the cardiac cycle.', estimatedMinutes: 20, preparation: 'outline', blocks: [], questions: [], completed: false };
const next: LearningLessonDto = { ...held, id: 'lesson-2', title: 'Blood vessels' };

function program(title: string, goal = GOAL): LearningProgramDto {
  return {
    outlineReview: null,
    summary: { id: 'program-1', title, goal, status: 'active', revision: 9, currentLessonId: 'lesson-1' },
    priorKnowledge: '', minutesPerSession: 30, modelName: 'Test model',
    modules: [{ id: 'module-1', title: 'Circulation', summary: 'How blood moves.', outcomes: ['Describe the cardiac cycle'], lessons: [held, next] }],
    sources: [], attempts: [],
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
      formFor={() => ({ answers: {}, attemptId: null, fingerprint: null })} setAnswer={vi.fn()} onSubmitAttempt={vi.fn()} submitPending={false}
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

  it('keeps a goal that differs from the title behind one Goal disclosure', async () => {
    show(program('Circulation'));
    expect(await screen.findByRole('heading', { level: 1, name: 'Circulation' })).toBeVisible();
    expect(screen.getByText('Goal')).toBeVisible();
    expect(screen.getAllByText(GOAL)).toHaveLength(1);
  });

  it('renders StudyHabitsBar and switches workbench mode between grounded practice and Feynman technique', async () => {
    const user = userEvent.setup();
    show(program('Circulation'));
    expect(await screen.findByRole('heading', { level: 1, name: 'Circulation' })).toBeVisible();

    // Navigate to Practice tab
    await user.click(screen.getByRole('tab', { name: 'Practice' }));

    // Mode switcher buttons exist
    const groundedBtn = screen.getByRole('button', { name: 'Grounded practice' });
    const feynmanBtn = screen.getByRole('button', { name: 'Feynman technique (Teach a beginner)' });
    expect(groundedBtn).toHaveAttribute('aria-pressed', 'true');
    expect(feynmanBtn).toHaveAttribute('aria-pressed', 'false');

    // Switch to Feynman mode
    await user.click(feynmanBtn);
    expect(groundedBtn).toHaveAttribute('aria-pressed', 'false');
    expect(feynmanBtn).toHaveAttribute('aria-pressed', 'true');
  });

  it('allows selecting Free recall (Brain dump) in the Recall tab', async () => {
    const user = userEvent.setup();
    show(program('Circulation'));
    expect(await screen.findByRole('heading', { level: 1, name: 'Circulation' })).toBeVisible();

    // Navigate to Recall tab
    await user.click(screen.getByRole('tab', { name: 'Recall' }));

    const brainDumpBtn = screen.getByRole('button', { name: 'Free recall (Brain dump)' });
    expect(brainDumpBtn).toHaveAttribute('aria-pressed', 'false');

    await user.click(brainDumpBtn);
    expect(brainDumpBtn).toHaveAttribute('aria-pressed', 'true');
  });
});
