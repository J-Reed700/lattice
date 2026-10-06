import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';


import { LEARNING_PROGRAMS_KEY, learningPlanKey, learningProgramKey } from '@/features/learning/api/learningQueryKeys';
import { useAcceptLearningCurriculumRevision, useLearningPlan } from '@/features/learning/curriculum/useLearningPlan';
import { useCompleteLearningLesson, useLearningProgram, usePrepareLearningLesson } from '@/features/learning/workspace/useLearningStudio';
import VaultAPI from '@/lib/api';
import type { LearningPlanDto, LearningProgramDto } from '@/lib/bindings';

vi.mock('@/lib/api', () => ({ default: {
  getLearningPlan: vi.fn(), getLearningProgram: vi.fn(), acceptLearningCurriculumRevision: vi.fn(),
  completeLearningLesson: vi.fn(), prepareLearningLesson: vi.fn(),
} }));

const ok = <T,>(data: T) => ({ ok: true as const, data });
const program: LearningProgramDto = {
  summary: { id: 'program', title: 'Evidence', goal: 'Learn', status: 'active', revision: 9,
    moduleCount: 1, lessonCount: 1, completedLessons: 0, currentLessonId: 'lesson', createdAt: 1 },
  priorKnowledge: '', minutesPerSession: 20, modelName: 'fixture', outlineReview: null,
  modules: [{ id: 'module', title: 'Foundations', summary: '', outcomes: [], prerequisiteModuleIds: [], project: null,
    lessons: [{ id: 'lesson', title: 'Before acceptance', objective: 'Learn', estimatedMinutes: 20,
      preparation: 'outline', blocks: [], questions: [], completed: false }] }],
  sources: [], attempts: [],
};
const plan: LearningPlanDto = { programId: 'program', programRevision: 9, acceptedRevision: null, draftRevision: null,
  previewChanges: [], requiredLessonCountBefore: 1, requiredLessonCountAfter: 1, resumeLessonId: 'lesson', jobs: [], latestDiagnostic: null };

function setup() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity }, mutations: { retry: false } } });
  client.setQueryData(learningProgramKey('program'), program);
  client.setQueryData(LEARNING_PROGRAMS_KEY, [program.summary]);
  client.setQueryData(learningPlanKey('program'), plan);
  return { client, wrapper: ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider> };
}

beforeEach(() => vi.resetAllMocks());

it('shows the saved preparation job after acknowledgement while the lesson remains an outline', async () => {
  const queued: LearningPlanDto = { ...plan, jobs: [{
    id: 'job', programId: 'program', operationId: 'prepare', kind: 'lesson_preparation',
    status: 'pending', baseRevisionNumber: 9, payloadSha256: 'fixture', resultId: null, retryOfJobId: null,
    progressCompleted: 0, progressTotal: 1, progressMessage: 'Request saved',
    createdAt: 1, startedAt: null, finishedAt: null, error: null, activity: null,
  }] };
  vi.mocked(VaultAPI.getLearningPlan).mockResolvedValue(ok(plan));
  vi.mocked(VaultAPI.prepareLearningLesson).mockImplementation(async () => {
    vi.mocked(VaultAPI.getLearningPlan).mockResolvedValue(ok(queued));
    return ok(program);
  });
  const { client, wrapper } = setup();
  const { result } = renderHook(() => ({
    program: useLearningProgram('program'), plan: useLearningPlan('program'), prepare: usePrepareLearningLesson(),
  }), { wrapper });
  await act(async () => { await result.current.prepare.mutateAsync({ programId: 'program', lessonId: 'lesson', expectedRevision: 9 }); });
  await waitFor(() => expect(result.current.plan.data?.jobs[0]?.status).toBe('pending'));
  expect(result.current.prepare.isPending).toBe(false);
  expect(result.current.program.data?.modules[0]?.lessons[0]?.preparation).toBe('outline');
  expect(client.getQueryState(LEARNING_PROGRAMS_KEY)?.isInvalidated).toBe(true);
  client.clear();
});

it('refreshes the mounted lesson screen before preparing work after curriculum acceptance', async () => {
  const next = structuredClone(program);
  next.summary.revision = 10;
  next.modules[0]!.lessons[0]!.title = 'Accepted lesson';
  vi.mocked(VaultAPI.getLearningProgram).mockResolvedValue(ok(next));
  vi.mocked(VaultAPI.acceptLearningCurriculumRevision).mockResolvedValue(ok({ ...plan, programRevision: 10 }));
  vi.mocked(VaultAPI.prepareLearningLesson).mockResolvedValue(ok(next));
  const { client, wrapper } = setup();
  const { result } = renderHook(() => ({ program: useLearningProgram('program'), accept: useAcceptLearningCurriculumRevision(), prepare: usePrepareLearningLesson() }), { wrapper });
  await act(async () => { await result.current.accept.mutateAsync({ operationId: 'accept', programId: 'program', revisionId: 'draft', expectedRevision: 9 }); });
  await waitFor(() => {
    expect(result.current.program.data?.summary.revision).toBe(10);
    expect(result.current.program.data?.modules[0]?.lessons[0]?.title).toBe('Accepted lesson');
  });
  expect(client.getQueryData(learningPlanKey('program'))).toEqual({ ...plan, programRevision: 10 });
  expect(client.getQueryState(LEARNING_PROGRAMS_KEY)?.isInvalidated).toBe(true);
  await act(async () => { await result.current.prepare.mutateAsync({ programId: 'program', lessonId: 'lesson', expectedRevision: result.current.program.data!.summary.revision }); });
  expect(VaultAPI.prepareLearningLesson).toHaveBeenCalledWith(expect.objectContaining({ expectedRevision: 10 }));
  client.clear();
});

it('refreshes published lesson content without depending on the visible progress panel', async () => {
  const ready = structuredClone(program);
  ready.summary.revision = 10;
  ready.modules[0]!.lessons[0]!.preparation = 'ready';
  vi.mocked(VaultAPI.getLearningPlan).mockResolvedValue(ok(plan));
  vi.mocked(VaultAPI.getLearningProgram).mockResolvedValue(ok(ready));
  const { client, wrapper } = setup();
  const { result } = renderHook(() => ({ program: useLearningProgram('program'), plan: useLearningPlan('program') }), { wrapper });
  await waitFor(() => expect(result.current.plan.isFetching).toBe(false));
  expect(result.current.program.data?.modules[0]?.lessons[0]?.preparation).toBe('outline');
  vi.mocked(VaultAPI.getLearningPlan).mockResolvedValue(ok({ ...plan, programRevision: 10 }));
  await act(async () => { await result.current.plan.refetch(); });
  await waitFor(() => expect(result.current.program.data?.modules[0]?.lessons[0]?.preparation).toBe('ready'));
  expect(result.current.program.data?.summary.revision).toBe(10);
  expect(client.getQueryState(LEARNING_PROGRAMS_KEY)?.isInvalidated).toBe(true);
  client.clear();
});

it('invalidates inactive program readers after curriculum acceptance', async () => {
  vi.mocked(VaultAPI.acceptLearningCurriculumRevision).mockResolvedValue(ok({ ...plan, programRevision: 10 }));
  const { client, wrapper } = setup();
  const { result } = renderHook(useAcceptLearningCurriculumRevision, { wrapper });
  await act(async () => { await result.current.mutateAsync({ operationId: 'accept', programId: 'program', revisionId: 'draft', expectedRevision: 9 }); });
  expect(client.getQueryState(learningProgramKey('program'))?.isInvalidated).toBe(true);
  expect(client.getQueryState(LEARNING_PROGRAMS_KEY)?.isInvalidated).toBe(true);
  client.clear();
});

it('refreshes the plan concurrency token when a lesson changes the program revision', async () => {
  vi.mocked(VaultAPI.getLearningPlan).mockResolvedValue(ok(plan));
  vi.mocked(VaultAPI.completeLearningLesson).mockImplementation(async () => {
    vi.mocked(VaultAPI.getLearningPlan).mockResolvedValue(ok({ ...plan, programRevision: 10 }));
    return ok({ ...program, summary: { ...program.summary, revision: 10, completedLessons: 1 } });
  });
  const { client, wrapper } = setup();
  const { result } = renderHook(() => ({ plan: useLearningPlan('program'), complete: useCompleteLearningLesson() }), { wrapper });
  await waitFor(() => expect(result.current.plan.isFetching).toBe(false));
  await act(async () => { await result.current.complete.mutateAsync({ programId: 'program', lessonId: 'lesson', expectedRevision: 9 }); });
  await waitFor(() => expect(result.current.plan.data?.programRevision).toBe(10));
  client.clear();
});
