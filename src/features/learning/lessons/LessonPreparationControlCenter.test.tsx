import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { LessonPreparationControlCenter } from '@/features/learning/lessons/LessonPreparationControlCenter';
import type { PreparationJobWithRank } from '@/features/learning/lessons/preparationControlModel';
import type {
  LearningGenerationActivity,
  LearningLessonDto,
  LearningModuleDto,
  LearningProgramDto,
} from '@/lib/bindings';

const api = vi.hoisted(() => ({
  prepareLessons: vi.fn(),
  cancelJob: vi.fn(),
  retryJob: vi.fn(),
  prioritizeJob: vi.fn(),
  resetPreparation: vi.fn(),
}));

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/api')>()),
  default: {
    prepareLearningLessons: api.prepareLessons,
    cancelLearningGenerationJob: api.cancelJob,
    retryLearningGenerationJob: api.retryJob,
    prioritizeLearningGenerationJob: api.prioritizeJob,
    resetLearningLessonPreparation: api.resetPreparation,
  },
}));

function makeLesson(
  id: string,
  title: string,
  preparation: 'outline' | 'ready' = 'outline',
): LearningLessonDto {
  return {
    id,
    title,
    objective: `Understand ${title}`,
    estimatedMinutes: 20,
    preparation,
    blocks: [],
    completed: false,
  } as unknown as LearningLessonDto;
}

function makeActivity(overrides: Partial<LearningGenerationActivity> = {}): LearningGenerationActivity {
  return {
    phase: 'evidence',
    phaseStartedAt: 1_000,
    lastActivityAt: 2_000,
    lastCheckpointAt: 1_500,
    lessonTitle: null,
    modelName: 'test-model',
    modelRunning: true,
    responseCharacters: 120,
    modelAttempt: 1,
    verificationPass: 1,
    checksCompleted: 14,
    checksTotal: 28,
    checksReused: 3,
    newReferences: null,
    statements: null,
    claims: null,
    recentSteps: [],
    ...overrides,
  };
}

function makeJob(overrides: Partial<PreparationJobWithRank> = {}): PreparationJobWithRank {
  return {
    id: 'job-1',
    programId: 'program-1',
    operationId: 'op-1',
    kind: 'lesson_preparation',
    payloadSha256: 'hash',
    baseRevisionNumber: 15,
    status: 'pending',
    progressCompleted: 0,
    progressTotal: 1,
    progressMessage: 'Queued',
    activity: null,
    resultId: null,
    error: null,
    errorCode: null,
    endedByUpdate: false,
    lessonIds: [],
    heldLessonIds: [],
    retryOfJobId: null,
    createdAt: 1_000,
    startedAt: null,
    finishedAt: null,
    ...overrides,
  };
}

const program: LearningProgramDto = {
  summary: {
    id: 'program-1',
    title: 'Distributed Systems',
    goal: 'Build reliable services',
    status: 'active',
    revision: 15,
    moduleCount: 2,
    lessonCount: 7,
    completedLessons: 0,
    currentLessonId: 'l-ready',
    createdAt: 1,
  },
  priorKnowledge: '',
  minutesPerSession: 30,
  modelName: 'test-model',
  outlineReview: null,
  sources: [],
  modules: [
    {
      id: 'mod-1',
      title: 'Consensus',
      summary: 'Agreement protocols',
      outcomes: [],
      prerequisiteModuleIds: [],
      project: null,
      lessons: [
        makeLesson('l-ready', 'Logical Clocks', 'ready'),
        makeLesson('l-running', 'Raft Leader Election'),
        makeLesson('l-queued', 'Log Replication'),
        makeLesson('l-paused', 'Snapshot Compaction'),
      ],
    },
    {
      id: 'mod-2',
      title: 'Storage Engines',
      summary: 'Persistent data structures',
      outcomes: [],
      prerequisiteModuleIds: [],
      project: null,
      lessons: [
        makeLesson('l-failed', 'LSM Trees'),
        makeLesson('l-review', 'Write-Ahead Logs'),
        makeLesson('l-fresh', 'Bloom Filters'),
      ],
    },
  ],
};

const jobs: PreparationJobWithRank[] = [
  makeJob({
    id: 'j-running',
    status: 'running',
    lessonIds: ['l-running'],
    createdAt: 100,
    rank: 1,
    startedAt: Date.now() - 15_000,
    progressMessage: 'Verifying statements',
    activity: makeActivity({ checksCompleted: 14, checksTotal: 28, checksReused: 3 }),
  }),
  makeJob({
    id: 'j-queued',
    status: 'pending',
    lessonIds: ['l-queued'],
    createdAt: 200,
    rank: 2,
    progressMessage: 'Waiting in queue',
  }),
  makeJob({
    id: 'j-paused',
    status: 'cancelled',
    lessonIds: ['l-paused'],
    createdAt: 300,
    activity: makeActivity({ phase: 'writing', checksCompleted: 0, checksTotal: 0, checksReused: 0 }),
  }),
  makeJob({
    id: 'j-failed',
    status: 'failed',
    lessonIds: ['l-failed'],
    createdAt: 400,
    error: 'Connection timed out',
  }),
  makeJob({
    id: 'j-review',
    status: 'completed',
    lessonIds: ['l-review'],
    heldLessonIds: ['l-review'],
    createdAt: 500,
  }),
];

function renderDrawer(overrides: Partial<{
  open: boolean;
  onOpenChange: (open: boolean) => void;
  waitsForIndex: string | null;
  onOpenLesson: (module: LearningModuleDto, lessonId: string) => void;
  onReview: (programId: string, lessonId: string) => void;
}> = {}) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const onOpenChange = overrides.onOpenChange ?? vi.fn();
  const onOpenLesson = overrides.onOpenLesson ?? vi.fn();
  const onReview = overrides.onReview ?? vi.fn();

  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );

  const view = render(
    <LessonPreparationControlCenter
      open={overrides.open ?? true}
      onOpenChange={onOpenChange}
      program={program}
      planJobs={jobs}
      programRevision={15}
      waitsForIndex={overrides.waitsForIndex ?? null}
      paceMinutes={4}
      onOpenLesson={onOpenLesson}
      onReview={onReview}
    />,
    { wrapper },
  );

  return { ...view, client, onOpenChange, onOpenLesson, onReview };
}

describe('LessonPreparationControlCenter', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    api.prepareLessons.mockResolvedValue({ ok: true, data: [] });
    api.cancelJob.mockResolvedValue({ ok: true, data: jobs[0] });
    api.retryJob.mockResolvedValue({ ok: true, data: jobs[2] });
    api.prioritizeJob.mockResolvedValue({ ok: true, data: jobs[1] });
    api.resetPreparation.mockResolvedValue({
      ok: true,
      data: { resetLessonIds: ['l-ready'], cancelledJobIds: [], programRevision: 16 },
    });
  });

  it('renders modules, all 7 statuses, readiness bar, pace line, and statement check progress', () => {
    const { client, unmount } = renderDrawer();

    expect(screen.getByRole('heading', { name: 'Lesson Preparation Control Center' })).toBeVisible();
    expect(screen.getByText('1 of 7 ready')).toBeVisible();
    expect(screen.getByText('About one lesson every 4 min so far')).toBeVisible();

    const mod1 = screen.getByRole('region', { name: 'Module 1: Consensus' });
    expect(within(mod1).getByText('1 of 4 ready')).toBeVisible();
    expect(within(mod1).getByText('1.1')).toBeVisible();
    expect(within(mod1).getByText('Logical Clocks')).toBeVisible();
    expect(within(mod1).getByText('Running')).toBeVisible();
    expect(within(mod1).getByText('Queued #2')).toBeVisible();
    expect(within(mod1).getByText('14/28')).toBeVisible();

    const mod2 = screen.getByRole('region', { name: 'Module 2: Storage Engines' });
    expect(within(mod2).getByText('0 of 3 ready')).toBeVisible();
    expect(within(mod2).getByText('Connection timed out')).toBeVisible();
    expect(within(mod2).getByText('Needs review')).toBeVisible();
    expect(within(mod2).getByText('Not started')).toBeVisible();

    unmount();
    client.clear();
  });

  it('filters by status pills and searches by lesson or module title', async () => {
    const user = userEvent.setup();
    const { client, unmount } = renderDrawer();

    await user.click(screen.getByRole('button', { name: /Active/i }));
    expect(screen.getByText('Raft Leader Election')).toBeVisible();
    expect(screen.getByText('Log Replication')).toBeVisible();
    expect(screen.queryByText('Logical Clocks')).not.toBeInTheDocument();
    expect(screen.queryByText('Bloom Filters')).not.toBeInTheDocument();

    await user.click(within(screen.getByRole('group', { name: 'Filter lessons by status' })).getByRole('button', { name: /^All/ }));
    const search = screen.getByRole('searchbox', { name: 'Filter lessons or modules' });
    await user.type(search, 'storage');
    expect(screen.getByText('LSM Trees')).toBeVisible();
    expect(screen.getByText('Bloom Filters')).toBeVisible();
    expect(screen.queryByText('Logical Clocks')).not.toBeInTheDocument();

    unmount();
    client.clear();
  });

  it('supports Select All, module indeterminate selection, and contextual bulk actions', async () => {
    const user = userEvent.setup();
    const { client, unmount } = renderDrawer();

    const masterCheckbox = screen.getByRole('checkbox', { name: 'Select all visible lessons' }) as HTMLInputElement;
    const mod1Checkbox = screen.getByRole('checkbox', { name: 'Select all lessons in Consensus' }) as HTMLInputElement;

    // Select one lesson in module 1 -> both module 1 and master checkboxes become indeterminate
    await user.click(screen.getByRole('checkbox', { name: 'Select Raft Leader Election' }));
    expect(mod1Checkbox.indeterminate).toBe(true);
    expect(masterCheckbox.indeterminate).toBe(true);

    // Select all visible lessons
    await user.click(masterCheckbox);
    expect(masterCheckbox.checked).toBe(true);
    expect(mod1Checkbox.checked).toBe(true);
    expect(mod1Checkbox.indeterminate).toBe(false);

    const bulkBar = screen.getByRole('region', { name: 'Bulk selection actions' });
    expect(within(bulkBar).getByText('7 selected')).toBeVisible();
    expect(within(bulkBar).getByRole('button', { name: 'Prepare (1)' })).toBeVisible();
    expect(within(bulkBar).getByRole('button', { name: 'Pause (2)' })).toBeVisible();
    expect(within(bulkBar).getByRole('button', { name: 'Resume (1)' })).toBeVisible();
    expect(within(bulkBar).getByRole('button', { name: 'Retry (1)' })).toBeVisible();
    expect(within(bulkBar).getByRole('button', { name: 'Reset / Delete (6)' })).toBeVisible();

    await user.click(within(bulkBar).getByRole('button', { name: 'Prepare (1)' }));
    expect(api.prepareLessons).toHaveBeenCalledWith(expect.objectContaining({
      programId: 'program-1',
      lessonIds: ['l-fresh'],
      expectedRevision: 15,
    }));

    await user.click(within(bulkBar).getByRole('button', { name: 'Pause (2)' }));
    expect(api.cancelJob).toHaveBeenCalledTimes(2);

    await user.click(within(bulkBar).getByRole('button', { name: 'Resume (1)' }));
    expect(api.retryJob).toHaveBeenCalledWith(expect.objectContaining({ jobId: 'j-paused' }));

    await user.click(within(bulkBar).getByRole('button', { name: 'Retry (1)' }));
    expect(api.retryJob).toHaveBeenCalledWith(expect.objectContaining({ jobId: 'j-failed' }));

    await user.click(screen.getByRole('button', { name: 'Clear' }));
    expect(screen.queryByRole('region', { name: 'Bulk selection actions' })).not.toBeInTheDocument();

    unmount();
    client.clear();
  });

  it('executes global Pause all, Resume all, Retry failed, and Prepare remaining', async () => {
    const user = userEvent.setup();
    const { client, unmount } = renderDrawer();

    await user.click(screen.getByRole('button', { name: 'Pause all' }));
    expect(api.cancelJob).toHaveBeenCalledTimes(2);

    await user.click(screen.getByRole('button', { name: 'Resume all' }));
    expect(api.retryJob).toHaveBeenCalledWith(expect.objectContaining({ jobId: 'j-paused' }));

    await user.click(screen.getByRole('button', { name: 'Retry failed' }));
    expect(api.retryJob).toHaveBeenCalledWith(expect.objectContaining({ jobId: 'j-failed' }));

    await user.click(screen.getByRole('button', { name: 'Prepare remaining' }));
    expect(api.prepareLessons).toHaveBeenCalledWith(expect.objectContaining({
      lessonIds: ['l-fresh'],
    }));

    unmount();
    client.clear();
  });

  it('disables Resume, Retry, and Prepare actions when waitsForIndex is active', () => {
    const { client, unmount } = renderDrawer({ waitsForIndex: 'Indexing course references…' });

    expect(screen.getByText('Indexing course references…')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Resume all' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Retry failed' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Prepare remaining' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Pause all' })).toBeEnabled();

    unmount();
    client.clear();
  });

  it('requires inline confirmation before Reset and executes resetLearningLessonPreparation on confirm', async () => {
    const user = userEvent.setup();
    const { client, unmount } = renderDrawer();

    await user.click(screen.getByRole('button', { name: 'Reset Logical Clocks' }));
    const confirmBanner = screen.getByRole('alertdialog', { name: 'Confirm preparation reset' });
    expect(within(confirmBanner).getByText(/Reset preparation for Logical Clocks\?/)).toBeVisible();

    await user.click(within(confirmBanner).getByRole('button', { name: 'Cancel' }));
    expect(api.resetPreparation).not.toHaveBeenCalled();
    expect(screen.queryByRole('alertdialog', { name: 'Confirm preparation reset' })).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Reset Logical Clocks' }));
    await user.click(screen.getByRole('button', { name: 'Confirm reset' }));

    await waitFor(() =>
      expect(api.resetPreparation).toHaveBeenCalledWith(expect.objectContaining({
        programId: 'program-1',
        lessonIds: ['l-ready'],
        expectedRevision: 15,
      })),
    );

    unmount();
    client.clear();
  });

  it('supports Run next prioritize, Review navigation, Open navigation, and inline error reporting', async () => {
    const user = userEvent.setup();
    const onOpenChange = vi.fn();
    const onOpenLesson = vi.fn();
    const onReview = vi.fn();
    const { client, unmount } = renderDrawer({ onOpenChange, onOpenLesson, onReview });

    await user.click(screen.getByRole('button', { name: 'Run next Log Replication' }));
    expect(api.prioritizeJob).toHaveBeenCalledWith(expect.objectContaining({
      programId: 'program-1',
      jobId: 'j-queued',
      expectedRevision: 15,
    }));

    // Error handling on failed action
    api.prioritizeJob.mockResolvedValueOnce({ ok: false, error: 'Queue priority locked' });
    await user.click(screen.getByRole('button', { name: 'Run next Log Replication' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Queue priority locked');
    await user.click(screen.getByRole('button', { name: 'Dismiss error' }));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();

    // Review navigation closes drawer
    await user.click(screen.getByRole('button', { name: 'Review Write-Ahead Logs' }));
    expect(onReview).toHaveBeenCalledWith('program-1', 'l-review');
    expect(onOpenChange).toHaveBeenCalledWith(false);

    // Lesson title navigation closes drawer
    await user.click(screen.getByRole('button', { name: 'Logical Clocks' }));
    expect(onOpenLesson).toHaveBeenCalledWith(program.modules[0], 'l-ready');
    expect(onOpenChange).toHaveBeenCalledWith(false);

    unmount();
    client.clear();
  });
});
