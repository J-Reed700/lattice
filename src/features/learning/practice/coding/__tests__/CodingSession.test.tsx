import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { codingApi, type CodingSessionDto } from '../codingApi';
import { CodingSession } from '../CodingSession';

// Mock codingApi
vi.mock('../codingApi', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../codingApi')>();
  return {
    ...actual,
    codingApi: {
      ...actual.codingApi,
      moveCodingStage: vi.fn(),
      checkCodingExample: vi.fn(),
      runCodingSession: vi.fn(),
      askCodingHint: vi.fn(),
      sendCodingTurn: vi.fn(),
      observeCodingDraft: vi.fn(),
      setCodingClock: vi.fn(),
      finishCodingSession: vi.fn(),
    },
  };
});

let rangeGeometry: { getClientRects?: PropertyDescriptor; getBoundingClientRect?: PropertyDescriptor } = {};

beforeAll(() => {
  rangeGeometry = {
    getClientRects: Object.getOwnPropertyDescriptor(Range.prototype, 'getClientRects'),
    getBoundingClientRect: Object.getOwnPropertyDescriptor(Range.prototype, 'getBoundingClientRect'),
  };
  Object.defineProperty(Range.prototype, 'getClientRects', { configurable: true, value: () => [] });
  Object.defineProperty(Range.prototype, 'getBoundingClientRect', {
    configurable: true,
    value: () => new DOMRect(0, 0, 0, 16),
  });
});

afterAll(() => {
  if (rangeGeometry.getClientRects) Object.defineProperty(Range.prototype, 'getClientRects', rangeGeometry.getClientRects);
  else Reflect.deleteProperty(Range.prototype, 'getClientRects');
  if (rangeGeometry.getBoundingClientRect) Object.defineProperty(Range.prototype, 'getBoundingClientRect', rangeGeometry.getBoundingClientRect);
  else Reflect.deleteProperty(Range.prototype, 'getBoundingClientRect');
});

const mockCoachedSession: CodingSessionDto = {
  id: 'session-coached-1',
  programId: 'program-1',
  activityId: 'activity-1',
  task: 'problem',
  setting: 'coached',
  stage: 'understand',
  budgetSeconds: null,
  activeMs: 45000,
  openedAt: 1_700_000_000_000,
  finishedAt: null,
  revision: 2,
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
        category: 'first repeat',
        args: ['"the cat sat on the mat"'],
        expected: '"the"',
        visibility: 'example',
      },
      {
        id: 'ex-2',
        category: 'no repeat',
        args: ['"Hello world"'],
        expected: 'None',
        visibility: 'example',
      },
    ],
    constraints: ['1 <= words <= 100,000'],
    subgoals: [{ id: 'sg-1', label: 'Case insensitive comparison' }],
    files: [
      {
        path: 'solution.py',
        role: 'starter',
        content: 'def first_repeated(sentence: str) -> str | None:\n    # Your code here\n    pass\n',
        contentSha256: 'sha-starter',
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
      content: 'def first_repeated(sentence: str) -> str | None:\n    # Your code here\n    pass\n',
    },
  ],
  examples: [
    {
      ordinal: 0,
      argsJson: '["the cat the dog"]',
      expectedJson: '"the"',
      verdict: 'right',
      catches: ['Compares with capitals'],
    },
  ],
  hints: [],
  availableNextRung: 'nudge',
  turns: [
    {
      id: 'turn-1',
      ordinal: 0,
      speaker: 'counterpart',
      content: 'Before any code: what should the function return when no word repeats?',
      citations: [],
      modelName: null,
      createdAt: 1_700_000_001_000,
    },
  ],
  runs: [],
  stageTimings: [
    { stage: 'understand', elapsedMs: 45000 },
  ],
  debrief: null,
};

const mockWatchedSession: CodingSessionDto = {
  ...mockCoachedSession,
  id: 'session-watched-2',
  setting: 'watched',
  budgetSeconds: 1800,
  activeMs: 600000,
  stage: 'code',
  problem: {
    ...mockCoachedSession.problem,
    title: 'Matching brackets',
    statement: 'Return True if every bracket is closed by the matching bracket.',
  },
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

describe('CodingSession', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders header with title, setting badge, help level, clock, and Finish button', () => {
    renderWithClient(<CodingSession session={mockCoachedSession} />);

    // Problem title
    expect(
      screen.getByRole('heading', { level: 1, name: 'First repeated word' }),
    ).toBeInTheDocument();

    // Setting badge
    expect(screen.getByText('Coached')).toBeInTheDocument();
    // Help level
    expect(screen.getByText('Full help')).toBeInTheDocument();

    // Finish button
    expect(screen.getByRole('button', { name: /^Finish$/i })).toBeInTheDocument();
  });

  it('renders soft budget clock in Watched setting', () => {
    renderWithClient(<CodingSession session={mockWatchedSession} />);

    expect(screen.getByText('Watched')).toBeInTheDocument();
    expect(screen.getByText('Hints only')).toBeInTheDocument();
    expect(screen.getByText(/of 30 min/i)).toBeInTheDocument();
  });

  it('CRITICAL (§4.6 & §9.5): LearningCodeEditor and RunBar remain usable and editable while counterpart turn is pending', () => {
    // When the thread is busy generating a reply ("Reading your code…")
    renderWithClient(
      <CodingSession
        session={mockCoachedSession}
      />,
    );

    // Code editor must not be marked readOnly or disabled
    const codeEditor = document.querySelector('.cm-content');
    expect(codeEditor).toBeInTheDocument();
    expect(codeEditor).not.toHaveAttribute('contenteditable', 'false');

    // Run bar buttons must remain enabled
    const runExamplesBtn = screen.getByRole('button', { name: /Run examples/i });
    expect(runExamplesBtn).not.toBeDisabled();

    const submitBtn = screen.getByRole('button', { name: /Submit/i });
    expect(submitBtn).not.toBeDisabled();
  });

  it('transitions stages via StageRail and calls moveCodingStage', async () => {
    const user = userEvent.setup();
    vi.mocked(codingApi.moveCodingStage).mockResolvedValueOnce({
      ...mockCoachedSession,
      stage: 'plan',
      revision: 3,
    });

    renderWithClient(<CodingSession session={mockCoachedSession} />);

    // Stage rail contains the 4 stages
    expect(screen.getByRole('button', { name: /Understand/i })).toBeInTheDocument();
    const planStageBtn = screen.getByRole('button', { name: /Plan/i });
    expect(planStageBtn).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Code/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Verify/i })).toBeInTheDocument();

    // Move to Plan stage
    await user.click(planStageBtn);

    expect(codingApi.moveCodingStage).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: 'session-coached-1',
        stage: 'plan',
        expectedRevision: 2,
      }),
    );
  });

  it('checks coding examples and displays verdict and misreading coverage', async () => {
    const user = userEvent.setup();
    vi.mocked(codingApi.checkCodingExample).mockResolvedValueOnce({
      ordinal: 1,
      argsJson: '["Go go"]',
      expectedJson: '"go"',
      verdict: 'right',
      catches: ['Compares with capitals'],
    });

    renderWithClient(<CodingSession session={mockCoachedSession} />);

    // Shows misreading coverage count
    expect(screen.getByText(/catch 1 of 3 common mistakes|Covers 1 of 3 common misreadings/i)).toBeInTheDocument();

    // Check an example
    const sentenceInput = screen.getByLabelText(/sentence/i);
    const expectedInput = screen.getByLabelText(/expected/i);
    const checkButton = screen.getByRole('button', { name: /^Check$/i });

    await user.type(sentenceInput, '"Go go"');
    await user.type(expectedInput, '"go"');
    await user.click(checkButton);

    expect(codingApi.checkCodingExample).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: 'session-coached-1',
        argsJson: '["Go go"]',
        expectedJson: '"go"',
      }),
    );
  });

  it('shows RunBar with keyboard shortcuts and calls runCodingSession with scopes', async () => {
    const user = userEvent.setup();
    vi.mocked(codingApi.runCodingSession).mockResolvedValue(mockCoachedSession);

    renderWithClient(<CodingSession session={mockCoachedSession} />);

    // Run examples with Ctrl+Enter shortcut hint
    const runExamplesBtn = screen.getByRole('button', { name: /Run examples/i });
    expect(runExamplesBtn).toHaveTextContent(/Ctrl\+Enter/i);
    await user.click(runExamplesBtn);

    expect(codingApi.runCodingSession).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: 'session-coached-1',
        scope: 'examples',
      }),
    );

    // Submit with Ctrl+Shift+Enter shortcut hint
    const submitBtn = screen.getByRole('button', { name: /Submit/i });
    expect(submitBtn).toHaveTextContent(/Ctrl\+Shift\+Enter/i);
    await user.click(submitBtn);

    expect(codingApi.runCodingSession).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: 'session-coached-1',
        scope: 'all',
      }),
    );
  });

  it('shows HintLadder in Coached and unlocks next rung on request', async () => {
    const user = userEvent.setup();
    vi.mocked(codingApi.askCodingHint).mockResolvedValueOnce({
      ...mockCoachedSession,
      hints: [
        {
          rung: 'nudge',
          content: 'When you reach a word, what do you need to know about the words before it?',
          reason: 'Encourage looking at historical occurrences',
        },
      ],
      availableNextRung: 'idea',
      revision: 3,
    });

    renderWithClient(<CodingSession session={mockCoachedSession} />);

    // Coached has HintLadder visible
    expect(screen.getByRole('heading', { level: 3, name: /Stuck\?/i })).toBeInTheDocument();
    expect(screen.getByText('Nudge')).toBeInTheDocument();
    expect(screen.getByText('Idea')).toBeInTheDocument();
    expect(screen.getByText('Plan')).toBeInTheDocument();
    expect(screen.getByText('Point at the line')).toBeInTheDocument();

    const nudgeButton = screen.getByRole('button', { name: /Get a nudge/i });
    await user.click(nudgeButton);

    expect(codingApi.askCodingHint).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: 'session-coached-1',
        expectedRevision: 2,
      }),
    );
  });

  it('hides progressive 4-rung HintLadder in Mock and Watched settings', () => {
    renderWithClient(<CodingSession session={mockWatchedSession} />);

    // In Watched setting, 4-rung progressive HintLadder is hidden
    expect(screen.queryByRole('heading', { level: 3, name: /Stuck\?/i })).not.toBeInTheDocument();
    expect(screen.queryByText('Get a nudge')).not.toBeInTheDocument();
    expect(screen.queryByText('Point at the line')).not.toBeInTheDocument();

    // Watched provides "I'm stuck" button in counterpart thread
    expect(screen.getByRole('button', { name: /I'm stuck/i })).toBeInTheDocument();
  });

  it('suggests mic and observes draft checkpoints in Watched setting', async () => {
    vi.useFakeTimers();
    vi.mocked(codingApi.observeCodingDraft).mockResolvedValue(mockWatchedSession);

    renderWithClient(<CodingSession session={mockWatchedSession} />);

    // PracticeThread hint in Watched
    expect(
      screen.getByText(/Talk through your thinking: type or use the mic/i),
    ).toBeInTheDocument();

    // Simulate typing in code editor
    const editor = document.querySelector('.cm-content');
    if (editor) {
      fireEvent.input(editor, { target: { textContent: 'def balanced(s): return True' } });
    }

    // Fast-forward debounce timer
    act(() => {
      vi.advanceTimersByTime(2500);
    });

    expect(codingApi.observeCodingDraft).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: 'session-watched-2',
      }),
    );

    vi.useRealTimers();
  });

  it('pauses and resumes clock on visibilitychange via set_coding_clock', () => {
    renderWithClient(<CodingSession session={mockWatchedSession} />);

    // Window hidden / tab switch
    Object.defineProperty(document, 'hidden', { value: true, configurable: true });
    fireEvent(document, new Event('visibilitychange'));

    expect(codingApi.setCodingClock).toHaveBeenCalledWith({
      sessionId: 'session-watched-2',
      open: false,
    });

    // Window visible / tab restored
    Object.defineProperty(document, 'hidden', { value: false, configurable: true });
    fireEvent(document, new Event('visibilitychange'));

    expect(codingApi.setCodingClock).toHaveBeenCalledWith({
      sessionId: 'session-watched-2',
      open: true,
    });
  });

  it('calls finishCodingSession on Finish click and triggers onFinish callback', async () => {
    const user = userEvent.setup();
    const onFinish = vi.fn();
    const finishedDto: CodingSessionDto = {
      ...mockCoachedSession,
      finishedAt: 1_700_000_200_000,
    };
    vi.mocked(codingApi.finishCodingSession).mockResolvedValueOnce(finishedDto);

    renderWithClient(
      <CodingSession
        session={mockCoachedSession}
        onFinish={onFinish}
      />,
    );

    const finishButton = screen.getByRole('button', { name: /^Finish$/i });
    await user.click(finishButton);

    expect(codingApi.finishCodingSession).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: 'session-coached-1',
        expectedRevision: 2,
      }),
    );
  });
});
