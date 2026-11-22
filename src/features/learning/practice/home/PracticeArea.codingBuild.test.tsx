import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { PROGRAM_KIND_ID } from '@/features/learning/practice/kinds';
import { programSession } from '@/features/learning/practice/kinds/program/__tests__/fixtures';
import { resetCodingBuildStore, useCodingBuildStore } from '@/features/learning/practice/kinds/program/codingBuildStore';
import type * as Wire from '@/lib/bindings';

import { PracticeArea } from './PracticeArea';
import { usePracticeView } from './usePracticeView';

const mocks = vi.hoisted(() => ({
  startBuild: vi.fn(),
  startSession: vi.fn(),
  listJobs: vi.fn(),
  subscribers: new Set<(job: Wire.JobDto) => void>(),
  openedSessions: [] as string[],
}));

vi.mock('@/lib/api', () => {
  let count = 0;
  return { newId: () => `id-${++count}`, unwrap: <T,>(value: T) => value, default: {}, VaultAPI: {} };
});
vi.mock('@/shared/ipc/transport', () => ({
  apiCall: (command: string) => {
    if (command === 'list_jobs') return mocks.listJobs();
    return Promise.resolve({ ok: true, data: [] });
  },
}));
vi.mock('@/features/jobs/api', () => ({
  useJobStatus: (subscriber: (job: Wire.JobDto) => void) => {
    mocks.subscribers.add(subscriber);
  },
  onJobStatus: (subscriber: (job: Wire.JobDto) => void) => {
    mocks.subscribers.add(subscriber);
    return () => {
      mocks.subscribers.delete(subscriber);
    };
  },
}));
vi.mock('@/features/learning/practice/kinds/program/programApi', () => ({
  programApi: { startBuild: mocks.startBuild },
}));
vi.mock('@/features/learning/practice/session/sessionApi', () => ({
  sessionApi: { startSession: mocks.startSession },
}));
vi.mock('./practiceHomeApi', () => ({
  practiceHomeApi: {
    getProfile: async () => ({ programId: 'course-1', details: '', language: null, interviewAt: null, company: null, role: null, revision: 1 }),
    saveProfile: vi.fn(),
    getFit: async () => ({ kinds: [{ kind: PROGRAM_KIND_ID, fit: 'fits', reason: null }] }),
    checkFit: async () => ({ kinds: [{ kind: PROGRAM_KIND_ID, fit: 'fits', reason: null }] }),
    due: async () => ({ items: [] }),
    codingSessions: async () => [],
    startPracticeSession: vi.fn(),
  },
}));
vi.mock('@/features/learning/practice/session/PracticeSession', () => ({
  PracticeSession: ({ sessionId }: { sessionId: string }) => {
    mocks.openedSessions.push(sessionId);
    return <div data-testid="practice-session">{`Session ${sessionId}`}</div>;
  },
}));
vi.mock('@/features/learning/practice/PracticeWorkbenchPanel', () => ({
  PracticeWorkbenchPanel: () => null,
}));
vi.mock('@/features/learning/practice/FeynmanTechniquePanel', () => ({
  FeynmanTechniquePanel: () => null,
}));
vi.mock('@/features/learning/practice/practical/PracticalWorkbenchPanel', () => ({
  PracticalWorkbenchPanel: () => null,
}));
vi.mock('@/features/learning/drills/CourseDrillsPanel', () => ({
  CourseDrillsPanel: () => null,
}));

const lesson = { id: 'lesson-1', title: 'Lesson 1' } as Wire.LearningLessonDto;
const program = {
  summary: { id: 'course-1', goal: 'Learn Python' },
  modules: [{ id: 'm1', lessons: [lesson] }],
} as unknown as Wire.LearningProgramDto;

function buildJob(overrides: Partial<Wire.JobDto> = {}): Wire.JobDto {
  return {
    id: 'job-old',
    kind: 'learning.practice_build',
    subjectId: 'course-1:coding',
    status: 'running',
    progressCurrent: 1,
    progressTotal: 3,
    progressMessage: '',
    activity: null,
    resultRef: null,
    errorCode: null,
    error: null,
    retryOfJobId: null,
    retryCount: 0,
    retryNotBefore: null,
    createdAt: 1,
    startedAt: 1,
    finishedAt: null,
    revision: 1,
    ...overrides,
  };
}

function publishJob(job: Wire.JobDto) {
  act(() => {
    for (const subscriber of [...mocks.subscribers]) subscriber(job);
  });
}

function Harness() {
  const view = usePracticeView();
  return <PracticeArea program={program} currentLesson={lesson} currentModule={undefined} view={view} />;
}

function renderArea() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: Infinity }, mutations: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <Harness />
    </QueryClientProvider>,
  );
}

describe('PracticeArea coding build lifecycle (real CodingEntry + store)', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    localStorage.clear();
    resetCodingBuildStore();
    mocks.subscribers.clear();
    mocks.openedSessions = [];
    mocks.listJobs.mockResolvedValue({ ok: true, data: [] });
  });

  it('discards the cancelled coding build handle so the next coding choice starts fresh with its new settings', async () => {
    const user = userEvent.setup();
    mocks.startBuild
      .mockResolvedValueOnce({ jobId: 'job-old' })
      .mockResolvedValueOnce({ jobId: 'job-new' });

    renderArea();
    const codeCard = await screen.findByTestId('card-code');
    await user.click(within(codeCard).getByRole('button', { name: 'Start Code' }));

    await waitFor(() =>
      expect(mocks.startBuild).toHaveBeenNthCalledWith(
        1,
        expect.objectContaining({ programId: 'course-1', difficulty: 'easy', form: 'build' }),
      ),
    );

    // Even if the backend job from the cancelled build is still reported as running,
    // Cancel must discard it and not reattach to it on the next coding start.
    mocks.listJobs.mockResolvedValue({ ok: true, data: [buildJob({ id: 'job-old', status: 'running' })] });
    await user.click(screen.getByRole('button', { name: 'Cancel' }));

    await screen.findByTestId('practice-home');
    expect(screen.queryByRole('region', { name: 'Coding build status' })).not.toBeInTheDocument();

    const cardAfterCancel = screen.getByTestId('card-code');
    await user.selectOptions(within(cardAfterCancel).getByRole('combobox', { name: 'Difficulty' }), 'hard');
    await user.selectOptions(within(cardAfterCancel).getByRole('combobox', { name: 'Task' }), 'repair');
    await user.click(within(cardAfterCancel).getByRole('button', { name: 'Start Code' }));

    await waitFor(() =>
      expect(mocks.startBuild).toHaveBeenNthCalledWith(
        2,
        expect.objectContaining({ programId: 'course-1', difficulty: 'hard', form: 'repair' }),
      ),
    );
  });

  it('does not let a stale ready coding handle hijack a later coding start', async () => {
    const user = userEvent.setup();
    let resolveOldSession!: (value: ReturnType<typeof programSession>) => void;
    mocks.startBuild
      .mockResolvedValueOnce({ jobId: 'job-old' })
      .mockResolvedValueOnce({ jobId: 'job-new' });
    mocks.startSession
      .mockImplementationOnce(() => new Promise((resolve) => { resolveOldSession = resolve; }))
      .mockResolvedValueOnce(programSession({ sessionId: 'session-new' }));

    renderArea();
    const codeCard = await screen.findByTestId('card-code');
    await user.click(within(codeCard).getByRole('button', { name: 'Start Code' }));
    await waitFor(() => expect(mocks.startBuild).toHaveBeenCalledTimes(1));

    // Old build completes and begins starting the session, then user leaves via "Back to Practice".
    publishJob(buildJob({ id: 'job-old', status: 'completed', progressCurrent: 3, resultRef: 'activity-old' }));
    await waitFor(() => expect(mocks.startSession).toHaveBeenCalledTimes(1));
    await user.click(screen.getByRole('button', { name: 'Back to Practice' }));
    await screen.findByTestId('practice-home');

    // Old session becomes ready while user is on PracticeHome.
    await act(async () => {
      resolveOldSession(programSession({ sessionId: 'session-old' }));
    });
    await waitFor(() =>
      expect(useCodingBuildStore.getState().handles['course-1']?.phase).toEqual({ kind: 'ready', sessionId: 'session-old' }),
    );
    expect(screen.getByRole('region', { name: 'Coding build status' })).toHaveTextContent('Your coding problem is ready.');

    // User starts a brand-new coding choice from the Code card without clicking "Open problem".
    const cardAgain = screen.getByTestId('card-code');
    await user.selectOptions(within(cardAgain).getByRole('combobox', { name: 'Difficulty' }), 'hard');
    await user.click(within(cardAgain).getByRole('button', { name: 'Start Code' }));

    await waitFor(() => expect(mocks.startBuild).toHaveBeenCalledTimes(2));
    expect(mocks.startBuild).toHaveBeenLastCalledWith(expect.objectContaining({ difficulty: 'hard' }));
    expect(mocks.openedSessions).not.toContain('session-old');
    expect(screen.queryByText('Session session-old')).not.toBeInTheDocument();

    publishJob(buildJob({ id: 'job-new', status: 'completed', progressCurrent: 3, resultRef: 'activity-new' }));
    expect(await screen.findByTestId('practice-session')).toHaveTextContent('Session session-new');
  });
});
