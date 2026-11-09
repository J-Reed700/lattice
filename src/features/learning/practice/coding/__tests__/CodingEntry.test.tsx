import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { codingApi, type CodingSessionDto } from '../codingApi';
import { CodingEntry, type CodingEntryRequest } from '../CodingEntry';

// Mock codingApi
vi.mock('../codingApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../codingApi')>();
  return {
    ...actual,
    codingApi: {
      ...actual.codingApi,
      startCodingBuild: vi.fn(),
      startCodingSession: vi.fn(),
      getCodingSession: vi.fn(),
      finishCodingSession: vi.fn(),
    },
  };
});

// Mock jobs api
const mockJobStatusSubscribers = new Set<(job: unknown) => void>();
vi.mock('@/features/jobs/api', () => ({
  JOB_STATUS_EVENT: 'jobs://status',
  onJobStatus: (subscriber: (job: unknown) => void) => {
    mockJobStatusSubscribers.add(subscriber);
    return () => mockJobStatusSubscribers.delete(subscriber);
  },
  useJobStatus: (subscriber: (job: unknown) => void) => {
    mockJobStatusSubscribers.add(subscriber);
  },
}));

const mockRequest: CodingEntryRequest = {
  task: 'problem',
  setting: 'coached',
  language: 'python',
  difficulty: 'easy',
  focus: null,
  budgetMinutes: null,
};

const mockSessionDto: CodingSessionDto = {
  id: 'session-123',
  programId: 'program-1',
  activityId: 'activity-1',
  task: 'problem',
  setting: 'coached',
  stage: 'understand',
  budgetSeconds: null,
  activeMs: 0,
  openedAt: 1_700_000_000_000,
  finishedAt: null,
  revision: 1,
  problem: {
    title: 'First repeated word',
    statement: 'Given a sentence, return the first word that appears a second time.',
    signature: {
      function: 'first_repeated',
      params: [{ name: 'sentence', annotation: 'str' }],
      returns: 'str | None',
    },
    examples: [
      {
        id: 'ex-1',
        category: 'basic repetition',
        args: ['"the cat the dog"'],
        expected: '"the"',
        visibility: 'example',
      },
    ],
    constraints: ['1 <= len(sentence) <= 100,000'],
    subgoals: [{ id: 'sg-1', label: 'Handle case sensitivity' }],
    files: [
      {
        path: 'solution.py',
        role: 'starter',
        content: 'def first_repeated(sentence: str) -> str | None:\n    pass\n',
        contentSha256: 'abc123',
        editable: true,
      },
    ],
    workedSolution: null,
    targetComplexity: 'O(n) time, O(n) space',
    totalMisreadings: 3,
  },
  draftFiles: [
    {
      path: 'solution.py',
      content: 'def first_repeated(sentence: str) -> str | None:\n    pass\n',
    },
  ],
  examples: [],
  hints: [],
  availableNextRung: 'nudge',
  turns: [],
  runs: [],
  stageTimings: [],
  debrief: null,
};

function renderWithClient(ui: ReactNode) {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: Infinity },
      mutations: { retry: false },
    },
  });
  return render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
}

describe('CodingEntry', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockJobStatusSubscribers.clear();
  });

  it('renders problem building checklist with progress bar and steps', async () => {
    vi.mocked(codingApi.startCodingBuild).mockResolvedValueOnce({ jobId: 'job-build-1' });

    renderWithClient(
      <CodingEntry
        programId="program-1"
        lessonId="lesson-1"
        request={mockRequest}
        onExit={vi.fn()}
      />,
    );

    // Headline and setting label
    expect(
      screen.getByRole('heading', { level: 1, name: /Building your problem/i }),
    ).toBeInTheDocument();
    expect(screen.getByText(/Code · Coached/i)).toBeInTheDocument();

    // Progress bar
    const progressbar = screen.getByRole('progressbar', { name: /Building your problem/i });
    expect(progressbar).toBeInTheDocument();
    expect(progressbar).toHaveAttribute('aria-valuemin', '0');
    expect(progressbar).toHaveAttribute('aria-valuemax', '3');

    // 3 named steps
    expect(screen.getByText(/Picked what to practise/i)).toBeInTheDocument();
    expect(screen.getByText(/Wrote the problem/i)).toBeInTheDocument();
    expect(screen.getByText(/Proving it/i)).toBeInTheDocument();

    // Leave-page reassurance notice
    expect(
      screen.getByText(/You can leave this page. It keeps building, and you'll find it under Continue./i),
    ).toBeInTheDocument();

    // Start button is disabled while building
    const startButton = screen.getByRole('button', { name: /^Start$/i });
    expect(startButton).toBeDisabled();
    expect(screen.getByText(/Ready when the proof finishes./i)).toBeInTheDocument();
  });

  it('calls onExit when clicking Back to Practice', async () => {
    const onExit = vi.fn();
    const user = userEvent.setup();

    renderWithClient(
      <CodingEntry
        programId="program-1"
        lessonId="lesson-1"
        request={mockRequest}
        onExit={onExit}
      />,
    );

    const backButton = screen.getByRole('button', { name: /Back to Practice/i });
    await user.click(backButton);

    expect(onExit).toHaveBeenCalledTimes(1);
  });

  it('enables Start button and announces status when problem build finishes', async () => {
    vi.mocked(codingApi.startCodingBuild).mockResolvedValueOnce({ jobId: 'job-build-2' });

    renderWithClient(
      <CodingEntry
        programId="program-1"
        lessonId="lesson-1"
        request={mockRequest}
        onExit={vi.fn()}
      />,
    );

    // Simulate job status event completing
    for (const sub of mockJobStatusSubscribers) {
      sub({
        id: 'job-build-2',
        kind: 'learning.coding_build',
        status: 'completed',
        progressCurrent: 3,
        progressTotal: 3,
        progressMessage: 'Done',
        activity: { activityId: 'activity-1', programId: 'program-1' },
        resultRef: JSON.stringify({ activityId: 'activity-1' }),
      });
    }

    await waitFor(() => {
      expect(screen.getByRole('heading', { level: 1, name: /Your problem is ready/i })).toBeInTheDocument();
    });

    const startButton = screen.getByRole('button', { name: /^Start$/i });
    expect(startButton).not.toBeDisabled();
    expect(screen.getByRole('status')).toHaveTextContent(/Your problem is ready/i);
  });

  it('transitions to CodingSession when Start is clicked after build', async () => {
    const user = userEvent.setup();
    vi.mocked(codingApi.startCodingBuild).mockResolvedValueOnce({ jobId: 'job-build-3' });
    vi.mocked(codingApi.startCodingSession).mockResolvedValueOnce(mockSessionDto);

    renderWithClient(
      <CodingEntry
        programId="program-1"
        lessonId="lesson-1"
        request={mockRequest}
        onExit={vi.fn()}
      />,
    );

    // Complete the build
    for (const sub of mockJobStatusSubscribers) {
      sub({
        id: 'job-build-3',
        kind: 'learning.coding_build',
        status: 'completed',
        progressCurrent: 3,
        progressTotal: 3,
        progressMessage: 'Done',
        activity: { activityId: 'activity-1', programId: 'program-1' },
      });
    }

    const startButton = await screen.findByRole('button', { name: /^Start$/i });
    await user.click(startButton);

    expect(codingApi.startCodingSession).toHaveBeenCalledWith(
      expect.objectContaining({
        programId: 'program-1',
        activityId: 'activity-1',
        setting: 'coached',
      }),
    );

    // Now inside CodingSession
    expect(await screen.findByRole('heading', { level: 1, name: 'First repeated word' })).toBeInTheDocument();
  });

  it('resumes existing session directly when sessionId is passed', async () => {
    vi.mocked(codingApi.getCodingSession).mockResolvedValueOnce(mockSessionDto);

    renderWithClient(
      <CodingEntry
        programId="program-1"
        lessonId="lesson-1"
        request={mockRequest}
        sessionId="session-123"
        onExit={vi.fn()}
      />,
    );

    expect(codingApi.getCodingSession).toHaveBeenCalledWith('session-123');
    expect(codingApi.startCodingBuild).not.toHaveBeenCalled();

    expect(await screen.findByRole('heading', { level: 1, name: 'First repeated word' })).toBeInTheDocument();
  });

  it('transitions to Debrief screen when session is finished', async () => {
    const finishedSession: CodingSessionDto = {
      ...mockSessionDto,
      finishedAt: 1_700_000_100_000,
      debrief: {
        sessionId: 'session-123',
        scorecard: {
          sessionId: 'session-123',
          rows: [
            {
              criterionId: 'problem_solving',
              title: 'Problem solving',
              description: 'Understood and clarified the problem.',
              status: 'observed',
              points: 3,
              maxPoints: 4,
              evidence: [{ kind: 'quote', text: 'Can the sentence be empty?' }],
              feedback: 'Asked clarifying questions early.',
              nextStep: 'State complexity during plan.',
            },
          ],
          graderModel: 'standard',
        },
        patterns: [
          {
            patternId: 'hash_set',
            name: 'Hash set lookup',
            family: 'lookup',
            recognitionCue: 'first repeated element',
            rating: 'good',
            dueAt: 1_700_500_000_000,
          },
        ],
        approachSummary: 'Single pass using a set.',
        targetComplexity: 'O(n) time, O(n) space',
        referenceSolution: 'def first_repeated(s):\n    pass\n',
        uncaughtMisreadings: [],
        stageTimings: [{ stage: 'understand', elapsedMs: 180000 }],
        runCount: 4,
        hintsUsed: ['nudge'],
        setting: 'coached',
      },
    };

    vi.mocked(codingApi.getCodingSession).mockResolvedValueOnce(finishedSession);

    renderWithClient(
      <CodingEntry
        programId="program-1"
        lessonId="lesson-1"
        request={mockRequest}
        sessionId="session-123"
        onExit={vi.fn()}
      />,
    );

    // Debrief screen elements
    expect(await screen.findByText(/DEBRIEF · CODE · COACHED/i)).toBeInTheDocument();
    expect(screen.getByText('Hash set lookup')).toBeInTheDocument();
    expect(screen.getByText('The strong answer')).toBeInTheDocument();
  });
});
