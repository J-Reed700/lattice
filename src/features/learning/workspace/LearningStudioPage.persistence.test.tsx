import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@/tests/interactionDom';
import { createMemoryRouter, RouterProvider, useLocation, useNavigate } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { programBuilderDraftKeys } from '@/features/learning/curriculum/programBuilderDraft';
import { resetPortabilityRunStore } from '@/features/learning/portability/portabilityRunStore';
import { LearningStudioPage } from '@/features/learning/workspace/LearningStudioPage';
import type {
  LearningOutlineProgressDto,
  LearningOutlineRunsSnapshotDto,
  LearningProgramDto,
  LearningProgramSummaryDto,
} from '@/lib/bindings';
import { queryClient as activityClient } from '@/lib/queryClient';
import { STUDY_ACTIVITY_KEY } from '@/lib/studyActivity';
import { readDraft } from '@/shared/drafts/drafts';
import { deferred } from '@/tests/deferred';
import { makeAppSettings } from '@/tests/fixtures/appSettings';
import { apiFailure, expectPlainError, findAlertWith } from '@/tests/plainErrors';

vi.mock('@/features/learning/practice/practical/PracticalWorkbenchPanel', () => ({
  PracticalWorkbenchPanel: () => null,
}));
vi.mock('@/features/learning/curriculum/PlanPanel', () => ({
  PlanPanel: () => <div data-testid="plan-panel">Plan view</div>,
}));
vi.mock('@/features/learning/canvas/CanvasPanel', () => ({
  default: () => <div data-testid="canvas-panel">Canvas view</div>,
}));
vi.mock('@/features/learning/portability/PortabilityPanel', () => ({
  PortabilityPanel: () => null,
}));
vi.mock('@/features/learning/portability/ImportCourseFlow', () => ({
  ImportCourseFlow: () => null,
}));
vi.mock('@/features/learning/drills/CourseDrillsPanel', () => ({
  CourseDrillsPanel: () => null,
}));

const mocks = vi.hoisted(() => ({
  list: vi.fn(),
  get: vi.fn(),
  generate: vi.fn(),
  repair: vi.fn(),
  cancel: vi.fn(),
  listRuns: vi.fn(),
  plan: vi.fn(),
  planCourse: vi.fn(),
  memory: vi.fn(),
}));

vi.mock('@/lib/pendingSaves', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/pendingSaves')>()),
  flushPendingSaves: async () => true,
}));

vi.mock('@/lib/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/api')>()),
  default: {
    listConversationSpaces: async () => ({ ok: true, data: [] }),
    listSpaceDocuments: async () => ({ ok: true, data: [] }),
    listLearningPrograms: mocks.list,
    getLearningProgram: mocks.get,
    getLearningOutlineEvidence: async () => ({ ok: true, data: null }),
    getLearningLessonEvidence: async () => ({ ok: true, data: null }),
    getLearningLessonClaimReview: async () => ({ ok: true, data: null }),
    generateLearningProgram: mocks.generate,
    repairLearningOutline: mocks.repair,
    cancelLearningOutline: mocks.cancel,
    listLearningOutlineRuns: mocks.listRuns,
    getLearningPlan: mocks.plan,
    planLearningCourse: mocks.planCourse,
    getLearningPracticeWorkspace: async (programId: string) => ({
      ok: true,
      data: { programId, sessions: [] },
    }),
    getLearningQuizOverview: async () => ({
      ok: true,
      data: {
        programId: 'program-1',
        settings: { quizLength: 5, gradingMode: 'strict' },
        lessonQuizzes: [],
        moduleTests: [],
        courseExam: null,
        followUps: [],
      },
    }),
    getLearningMemory: mocks.memory,
    listStudyDecks: async () => ({ ok: true, data: [] }),
    courseIndexOpen: async () => ({ ok: true, data: null }),
    courseIndexClose: async () => ({ ok: true, data: null }),
    getSettings: async () => ({ ok: true, data: settingsWithLearningModel() }),
    getDownloadedModels: async () => ({ ok: true, data: [] }),
  },
}));

function settingsWithLearningModel() {
  const base = makeAppSettings();
  return {
    ...base,
    llm: {
      ...base.llm,
      learningModel: { kind: 'cloud', provider: 'anthropic', model: 'claude-test' },
    },
  };
}

function ok<T>(data: T) {
  return { ok: true as const, data };
}

function makeDraftProgram(id = 'program-1'): LearningProgramDto {
  const summary: LearningProgramSummaryDto = {
    id,
    title: 'A thoughtful course',
    goal: 'Master C# and .NET internals',
    status: 'draft',
    revision: 0,
    moduleCount: 1,
    lessonCount: 1,
    completedLessons: 0,
    currentLessonId: 'lesson-1',
    createdAt: 1_790_000_000_000,
  };
  return {
    summary,
    priorKnowledge: '',
    minutesPerSession: 45,
    modelName: 'Test model',
    modules: [
      {
        id: 'module-1',
        title: 'Foundations',
        summary: 'Core runtime concepts.',
        outcomes: ['Explain CLR memory layout'],
        lessons: [
          {
            id: 'lesson-1',
            title: 'CLR Basics',
            objective: 'Understand the managed heap',
            estimatedMinutes: 45,
            preparation: 'outline',
            completed: false,
            blocks: [],
          } as LearningProgramDto['modules'][number]['lessons'][number],
        ],
      },
    ],
    sources: [],
  } as unknown as LearningProgramDto;
}

function NavShell({ children }: { children?: React.ReactNode }) {
  const navigate = useNavigate();
  const location = useLocation();
  return (
    <div>
      <nav aria-label="App navigation">
        <button type="button" onClick={() => void navigate('/studio')}>
          Go to Studio
        </button>
        <button type="button" onClick={() => void navigate('/settings')}>
          Go to Settings
        </button>
        <button type="button" onClick={() => void navigate('/studio?builder=1')}>
          Open Builder Activity
        </button>
      </nav>
      <span data-testid="location-search">{location.search}</span>
      {children}
    </div>
  );
}

function renderStudioApp(initialPath = '/studio', existingClient?: QueryClient) {
  const client =
    existingClient ??
    new QueryClient({
      defaultOptions: {
        queries: { retry: false, gcTime: Infinity },
        mutations: { retry: false },
      },
    });
  const router = createMemoryRouter(
    [
      {
        path: '/studio',
        element: (
          <NavShell>
            <LearningStudioPage />
          </NavShell>
        ),
      },
      {
        path: '/settings',
        element: (
          <NavShell>
            <div data-testid="settings-screen">Settings</div>
          </NavShell>
        ),
      },
    ],
    { initialEntries: [initialPath] },
  );
  const view = render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { client, router, ...view };
}

describe('Learning Studio course generation persistence and rehydration', () => {
  afterEach(() => {
    activityClient.removeQueries({ queryKey: STUDY_ACTIVITY_KEY });
  });

  beforeEach(() => {
    vi.resetAllMocks();
    resetPortabilityRunStore();
    localStorage.clear();
    mocks.list.mockResolvedValue(ok([]));
    mocks.listRuns.mockResolvedValue(
      ok<LearningOutlineRunsSnapshotDto>({ active: [], recentOutcomes: [] }),
    );
    mocks.plan.mockResolvedValue(ok({ programId: 'program-1', jobs: [] }));
    mocks.planCourse.mockResolvedValue(ok({ programId: 'program-1', entries: [] }));
    mocks.memory.mockResolvedValue(
      ok({
        programId: 'program-1',
        journalId: null,
        lessonNotes: [],
        studyDeck: null,
        drafts: [],
        acceptedCards: [],
        dueCount: 0,
      }),
    );
  });

  it('keeps active generation live across route unmount and remount with inputs and cancel ID intact', async () => {
    const user = userEvent.setup();
    let emitProgress!: (update: LearningOutlineProgressDto) => void;
    const generation = deferred<ReturnType<typeof ok<LearningProgramDto>>>();
    mocks.generate.mockImplementation((_request, options) => {
      emitProgress = options.onProgress;
      return generation.promise;
    });
    mocks.cancel.mockResolvedValue(ok(true));

    renderStudioApp('/studio?new=1');

    await user.type(await screen.findByLabelText('Your goal'), 'Master distributed consensus');
    await user.click(screen.getByRole('radio', { name: /Deep dive/ }));
    fireEvent.change(screen.getByLabelText('Time for each session'), {
      target: { value: '45' },
    });
    expect(screen.getByText('45 minutes')).toBeVisible();

    await user.click(screen.getByRole('button', { name: 'Create outline' }));
    const originalRequestId = mocks.generate.mock.calls[0][1].requestId as string;

    await act(async () => {
      emitProgress({
        stage: 'gathering_references',
        detail: 'Subtopic 3 of 10: Paxos vs Raft',
        elapsedSeconds: 42,
        stageSeconds: 18,
        responseCharacters: 0,
        modelName: 'Test model',
      });
    });

    await waitFor(() =>
      expect(screen.getByRole('status')).toHaveTextContent('Gathering references'),
    );
    expect(screen.getByLabelText('Stage progress')).toHaveTextContent('Subtopic 3 of 10');

    // Navigate away from /studio, unmounting LearningStudioPage completely.
    await user.click(screen.getByRole('button', { name: 'Go to Settings' }));
    expect(await screen.findByTestId('settings-screen')).toBeVisible();
    expect(screen.queryByRole('status')).not.toBeInTheDocument();

    // Navigate back to /studio: lands straight back on ProgramBuilder with live state.
    await user.click(screen.getByRole('button', { name: 'Go to Studio' }));
    expect(await screen.findByRole('status')).toHaveTextContent('Gathering references');
    expect(screen.getByLabelText('Stage progress')).toHaveTextContent('Subtopic 3 of 10');
    expect(screen.getByRole('radio', { name: /Deep dive/ })).toBeChecked();
    expect(screen.getByText('45-minute sessions')).toBeVisible();

    await user.click(screen.getByRole('button', { name: 'Cancel generation' }));
    expect(mocks.cancel).toHaveBeenCalledWith(originalRequestId);
  });

  it('allows browsing ProgramLibrary while generation runs and prevents duplicate submission', async () => {
    const user = userEvent.setup();
    const draft = makeDraftProgram('program-1');
    let emitProgress!: (update: LearningOutlineProgressDto) => void;
    const generation = deferred<ReturnType<typeof ok<LearningProgramDto>>>();
    mocks.generate.mockImplementation((_request, options) => {
      emitProgress = options.onProgress;
      return generation.promise;
    });
    mocks.get.mockResolvedValue(ok(draft));

    renderStudioApp('/studio?new=1');
    await user.type(await screen.findByLabelText('Your goal'), 'Understand compilers');
    await user.click(screen.getByRole('button', { name: 'Create outline' }));

    const backButton = screen.getByRole('button', { name: '← Back to programs' });
    expect(backButton).toBeEnabled();
    await user.click(backButton);

    const statusRegion = await screen.findByRole('region', {
      name: 'Course generation in progress',
    });
    expect(statusRegion).toBeVisible();
    expect(
      screen.queryByRole('button', { name: 'Build a program' }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Build your first program' }),
    ).not.toBeInTheDocument();
    expect(
      screen.getAllByRole('button', { name: /View outline progress/ }),
    ).toHaveLength(2);

    // Emitting a progress update with programId immediately invalidates programs list and updates banner.
    const listCallsBefore = mocks.list.mock.calls.length;
    mocks.list.mockResolvedValue(ok([draft.summary]));
    await act(async () => {
      emitProgress({
        programId: 'program-1',
        stage: 'checking_citations',
        detail: 'Linked 4 outline items',
        elapsedSeconds: 95,
        stageSeconds: 12,
        responseCharacters: 120,
        modelName: 'Test model',
      });
    });

    await waitFor(() => expect(mocks.list.mock.calls.length).toBeGreaterThan(listCallsBefore));
    expect(within(statusRegion).getByRole('status')).toHaveTextContent('Linking references');
    expect(within(statusRegion).getByLabelText('Stage progress')).toHaveTextContent(
      'Linked 4 outline items',
    );
    expect(
      within(statusRegion).getByRole('button', { name: 'Inspect saved draft' }),
    ).toBeVisible();

    await user.click(within(statusRegion).getByRole('button', { name: 'View progress' }));
    expect(await screen.findByRole('heading', { name: 'What would you like to learn?' })).toBeVisible();
    expect(screen.getByRole('status')).toHaveTextContent('Linking references');
  });

  it('surfaces background failures in ProgramLibrary and returns to builder with inputs intact', async () => {
    const user = userEvent.setup();
    const generation = deferred<ReturnType<typeof apiFailure>>();
    mocks.generate.mockReturnValue(generation.promise);

    renderStudioApp('/studio?new=1');
    await user.type(await screen.findByLabelText('Your goal'), 'Build a distributed cache');
    await user.click(screen.getByRole('radio', { name: /Deep dive/ }));
    fireEvent.change(screen.getByLabelText('Time for each session'), {
      target: { value: '45' },
    });
    await user.click(screen.getByRole('button', { name: 'Create outline' }));

    await user.click(screen.getByRole('button', { name: '← Back to programs' }));
    expect(
      await screen.findByRole('region', { name: 'Course generation in progress' }),
    ).toBeVisible();

    await act(async () => {
      generation.resolve(
        apiFailure('NETWORK_ERROR', 'Network error', 'connection refused (os error 61)'),
      );
    });

    const alert = await findAlertWith("Lattice couldn't reach the model server.");
    await expectPlainError(alert, {
      headline: "Lattice couldn't reach the model server.",
      raw: 'connection refused (os error 61)',
    });

    await user.click(screen.getByRole('button', { name: 'Return to course builder' }));

    expect(await screen.findByLabelText('Your goal')).toHaveValue('Build a distributed cache');
    expect(screen.getByRole('radio', { name: /Deep dive/ })).toBeChecked();
    expect(screen.getByText('45 minutes')).toBeVisible();
    await findAlertWith("Lattice couldn't reach the model server.");
    expect(readDraft(programBuilderDraftKeys.goal)).toBe('Build a distributed cache');
  });

  it('rehydrates an active backend run after reload, cancels with backend requestId, and opens finished draft', async () => {
    const user = userEvent.setup();
    const draft = makeDraftProgram('program-1');
    let snapshot: LearningOutlineRunsSnapshotDto = {
      active: [
        {
          requestId: 'backend-run-42',
          kind: 'generate',
          goal: 'Master C# and .NET internals',
          courseDepth: 'deep_dive',
          progress: {
            programId: null,
            stage: 'drafting',
            detail: null,
            waiting: null,
            elapsedSeconds: 504,
            stageSeconds: 30,
            responseCharacters: 1200,
            modelName: 'Test model',
          },
        },
      ],
      recentOutcomes: [],
    };
    mocks.listRuns.mockImplementation(async () => ok(snapshot));
    mocks.cancel.mockResolvedValue(ok(true));
    mocks.get.mockResolvedValue(ok(draft));

    renderStudioApp('/studio');

    expect(
      await screen.findByRole('heading', { name: 'What would you like to learn?' }),
    ).toBeVisible();
    expect(screen.getByRole('status')).toHaveTextContent('Writing your course outline');
    expect(screen.getByLabelText('Elapsed time')).toHaveTextContent('8:24');
    expect(screen.getByRole('radio', { name: /Deep dive/ })).toBeChecked();
    expect(screen.getByText(/A deep dive plans 24–60 lessons/)).toBeVisible();
    expect(screen.getByRole('heading', { name: 'Deep dive' })).toBeVisible();

    await user.click(screen.getByRole('button', { name: 'Cancel generation' }));
    expect(mocks.cancel).toHaveBeenCalledWith('backend-run-42');

    // Simulate backend finishing into recentOutcomes on the next poll.
    mocks.list.mockResolvedValue(ok([draft.summary]));
    snapshot = {
      active: [],
      recentOutcomes: [
        {
          requestId: 'backend-run-42',
          kind: 'generate',
          programId: 'program-1',
          cancelled: false,
          error: null,
        },
      ],
    };

    expect(
      await screen.findByRole('textbox', { name: 'Program title' }, { timeout: 4000 }),
    ).toHaveValue('A thoughtful course');
  });

  it('opens ProgramBuilder via ?builder=1 without resetting active generation and cleans the URL', async () => {
    const user = userEvent.setup();
    let emitProgress!: (update: LearningOutlineProgressDto) => void;
    const generation = deferred<ReturnType<typeof ok<LearningProgramDto>>>();
    mocks.generate.mockImplementation((_request, options) => {
      emitProgress = options.onProgress;
      return generation.promise;
    });
    mocks.cancel.mockResolvedValue(ok(true));

    renderStudioApp('/studio?new=1');
    await user.type(await screen.findByLabelText('Your goal'), 'Learn category theory');
    await user.click(screen.getByRole('button', { name: 'Create outline' }));
    const originalRequestId = mocks.generate.mock.calls[0][1].requestId as string;

    await act(async () => {
      emitProgress({
        stage: 'reviewing',
        detail: 'Checking module prerequisites',
        elapsedSeconds: 60,
        stageSeconds: 10,
        responseCharacters: 300,
        modelName: 'Test model',
      });
    });

    // Leave builder to library so builderFocused is false, then open /studio?builder=1 from global activity.
    await user.click(screen.getByRole('button', { name: '← Back to programs' }));
    expect(
      await screen.findByRole('region', { name: 'Course generation in progress' }),
    ).toBeVisible();

    await user.click(screen.getByRole('button', { name: 'Open Builder Activity' }));

    expect(await screen.findByRole('heading', { name: 'What would you like to learn?' })).toBeVisible();
    expect(screen.getByRole('status')).toHaveTextContent('Reviewing the teaching plan');
    await waitFor(() => expect(screen.getByTestId('location-search')).toHaveTextContent(''));

    await user.click(screen.getByRole('button', { name: 'Cancel generation' }));
    expect(mocks.cancel).toHaveBeenCalledWith(originalRequestId);
  });

  it('recovers a reload-gap failure once even with a 5-minute QueryClient staleTime and does not re-surface after dismiss', async () => {
    const user = userEvent.setup();
    const client = new QueryClient({
      defaultOptions: {
        queries: { retry: false, gcTime: Infinity, staleTime: 5 * 60_000 },
        mutations: { retry: false },
      },
    });
    mocks.listRuns.mockResolvedValue(
      ok<LearningOutlineRunsSnapshotDto>({
        active: [],
        recentOutcomes: [
          {
            requestId: 'reload-gap-run-1',
            kind: 'generate',
            programId: null,
            cancelled: false,
            error: {
              code: 'NETWORK_ERROR',
              message: 'Network error',
              details: 'connection dropped while reloading',
            },
          },
        ],
      }),
    );

    renderStudioApp('/studio', client);

    expect(
      await screen.findByRole('heading', { name: 'What would you like to learn?' }),
    ).toBeVisible();
    const alert = await findAlertWith("Lattice couldn't reach the model server.");
    await expectPlainError(alert, {
      headline: "Lattice couldn't reach the model server.",
      raw: 'connection dropped while reloading',
    });

    await user.click(screen.getByRole('button', { name: 'Dismiss error' }));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();

    // Unmount and remount /studio within the 5-minute staleTime window:
    // listRuns still refetches on mount, and the acknowledged failure stays dismissed.
    await user.click(screen.getByRole('button', { name: 'Go to Settings' }));
    expect(await screen.findByTestId('settings-screen')).toBeVisible();

    await user.click(screen.getByRole('button', { name: 'Go to Studio' }));
    expect(await screen.findByRole('heading', { name: 'Your programs' })).toBeVisible();
    await waitFor(() => expect(mocks.listRuns).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });
});
