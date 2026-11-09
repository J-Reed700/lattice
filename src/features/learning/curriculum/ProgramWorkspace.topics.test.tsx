import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { learningCoursePlanKey } from '@/features/learning/api/learningQueryKeys';
import { emptyPlan, entry, ok, planView, topics } from '@/features/learning/curriculum/coursePlan/coursePlanFixtures';
import { ProgramWorkspace } from '@/features/learning/curriculum/ProgramWorkspace';
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
vi.mock('@/features/reading/components/CitationVisibilityToggle', () => ({ CitationVisibilityToggle: () => null }));
vi.mock('@/features/learning/memory/MemoryPanels', async (importOriginal) => ({
  ...await importOriginal<typeof import('@/features/learning/memory/MemoryPanels')>(),
  NotebookPanel: () => null,
}));

const api = vi.hoisted(() => ({ plan: vi.fn(), coursePlan: vi.fn(), planCourse: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: {
  getLearningPlan: api.plan,
  getLearningCoursePlan: api.coursePlan,
  planLearningCourse: api.planCourse,
  getLearningLessonClaimReview: async () => ({ ok: true, data: null }),
  getLearningLessonEvidence: async () => ({ ok: true, data: null }),
  getLearningOutlineEvidence: async () => ({ ok: true, data: null }),
  getLearningMemory: async () => ({ ok: true, data: { dueCount: 0 } }),
  getLearningQuizOverview: async () => ({ ok: true, data: quizOverview() }),
  getLearningRecallWorkspace: async () => ({ ok: true, data: { programId: 'program-1', cards: [], duplicates: [], schedulerDisclosure: '' } }),
  getLearningSourceWorkspace: async () => ({ ok: true, data: { programId: 'program-1', sources: [] } }),
  courseIndexStatus: async () => ({ ok: true, data: { state: 'ready', mode: 'hybrid' } }),
  courseIndexOpen: async () => ({ ok: true, data: undefined }),
  courseIndexClose: async () => ({ ok: true, data: undefined }),
} }));

const lessonOf = (id: string, title: string, preparation: 'outline' | 'ready'): LearningLessonDto => (
  { id, title, objective: `Objective of ${title}`, estimatedMinutes: 20, preparation, blocks: [], completed: false } as unknown as LearningLessonDto
);

function course(status: 'draft' | 'active'): LearningProgramDto {
  return {
    outlineReview: null,
    summary: { id: 'program-1', title: 'Circulation', goal: 'Circulation', status, revision: 9, currentLessonId: 'lesson-1' },
    priorKnowledge: '', minutesPerSession: 30, modelName: 'Test model',
    modules: [{ id: 'module-1', title: 'The pump', summary: 'How blood moves.', outcomes: ['Describe a beat'], lessons: [lessonOf('lesson-1', 'The heart', 'outline'), lessonOf('lesson-2', 'Valves', 'ready')] }],
    sources: [],
  } as unknown as LearningProgramDto;
}

let lastClient: QueryClient;

const planned = () => planView([
  entry('lesson-1', { topics: topics('lesson-1', { ideas: ['Beat cycle', 'Chambers'], terms: ['Systole'] }) }),
  entry('lesson-2', { topics: topics('lesson-2', { ideas: ['Valve ideas'] }) }),
]);

const job = (overrides: Partial<LearningGenerationJob>): LearningGenerationJob => ({
  id: 'job-1', programId: 'program-1', operationId: 'op', kind: 'lesson_preparation', payloadSha256: 'hash', baseRevisionNumber: 9,
  status: 'running', progressCompleted: 0, progressTotal: 1, progressMessage: 'Working', activity: null,
  resultId: null, error: null, retryOfJobId: null, createdAt: Date.now() - 60_000, startedAt: Date.now() - 60_000, finishedAt: null,
  ...overrides,
});

function show(program: LearningProgramDto) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  lastClient = client;
  render(<QueryClientProvider client={client}>
    <ProgramWorkspace program={program} onBack={vi.fn()} onOpenProgram={vi.fn()} onAccept={vi.fn()} acceptPending={false}
      onPrepare={vi.fn()} preparePending={false} onPrepareModule={vi.fn()} onComplete={vi.fn()} completePending={false} />
  </QueryClientProvider>);
}

describe('planned topics on the course page', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.plan.mockResolvedValue(ok({ programId: 'program-1', jobs: [] }));
    api.coursePlan.mockResolvedValue(ok(planned()));
  });

  it('shows each lesson\'s ideas under its objective in the draft outline', async () => {
    show(course('draft'));
    expect(await screen.findByText('Beat cycle · Chambers')).toBeVisible();
    expect(screen.getByText('Valve ideas')).toBeVisible();
  });

  it('shows ideas on an active course\'s unprepared lessons and nothing on a ready one', async () => {
    show(course('active'));
    const unprepared = (await screen.findByRole('button', { name: /^The heart/ })).closest('article') as HTMLElement;
    expect(await within(unprepared).findByText('Beat cycle · Chambers')).toBeVisible();
    const ready = screen.getByRole('button', { name: /^Valves/ }).closest('article') as HTMLElement;
    expect(within(ready).queryByText('Valve ideas')).not.toBeInTheDocument();
  });

  it('says the topics are being planned, with no plan shown yet', async () => {
    api.coursePlan.mockResolvedValue(ok(emptyPlan('planning')));
    show(course('draft'));
    expect(await screen.findByText('Planning topics…')).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Review topics' })).not.toBeInTheDocument();
  });

  it('offers Plan topics when no plan exists and plans the course on click', async () => {
    const user = userEvent.setup();
    api.coursePlan.mockResolvedValue(ok(emptyPlan('none')));
    api.planCourse.mockResolvedValue(ok(emptyPlan('planning')));
    show(course('draft'));
    await user.click(await screen.findByRole('button', { name: 'Plan topics' }));
    await waitFor(() => expect(api.planCourse).toHaveBeenCalledWith({ programId: 'program-1' }));
  });

  it('watches the plan with a handful of observers however many lessons the outline lists', async () => {
    const base = course('draft');
    const many = ['lesson-1', 'lesson-2', 'lesson-3', 'lesson-4', 'lesson-5', 'lesson-6'].map((id) => lessonOf(id, `Lesson ${id}`, 'outline'));
    api.coursePlan.mockResolvedValue(ok(planView(many.map((lesson) => entry(lesson.id, { topics: topics(lesson.id, { ideas: [`Ideas of ${lesson.id}`] }) })))));
    show({ ...base, modules: [{ ...base.modules[0], lessons: many }] });
    expect(await screen.findByText('Ideas of lesson-6')).toBeVisible();
    expect(lastClient.getQueryCache().find({ queryKey: learningCoursePlanKey('program-1') })?.getObserversCount()).toBeLessThanOrEqual(2);
  });

  it('opens the Review topics screen from the course header once the topics are planned', async () => {
    const user = userEvent.setup();
    show(course('active'));
    await user.click(await screen.findByRole('button', { name: 'Review topics' }));
    expect(await screen.findByRole('heading', { name: 'Review topics' })).toBeVisible();
    expect(screen.getByRole('article', { name: 'The heart' })).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Back' }));
    expect(await screen.findByRole('button', { name: 'Review topics' })).toBeVisible();
  });

  it('does not count a running course-plan job as lesson preparation', async () => {
    api.plan.mockResolvedValue(ok({ programId: 'program-1', jobs: [job({ id: 'plan-job', kind: 'course_plan' as LearningGenerationJob['kind'], status: 'running' })] }));
    show(course('active'));
    expect(await screen.findByRole('button', { name: 'Prepare all lessons' })).toBeEnabled();
    expect((await screen.findAllByRole('button', { name: 'Prepare lesson' })).length).toBeGreaterThan(0);
    expect(screen.queryByRole('region', { name: 'Lesson preparation' })).not.toBeInTheDocument();
    expect(screen.queryByText(/Queued/)).not.toBeInTheDocument();
  });
});
