import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { CodingDebriefDto } from '../codingApi';
import { Debrief } from '../Debrief';

// Mock SaveAsReviewCard hook
vi.mock('@/features/learning/memory/useLearningMemory', () => ({
  useSaveLearningCardDraft: () => ({
    mutate: vi.fn(),
    isPending: false,
    isSuccess: false,
  }),
}));

const mockDebriefDto: CodingDebriefDto = {
  sessionId: 'session-debrief-1',
  scorecard: {
    sessionId: 'session-debrief-1',
    rows: [
      {
        criterionId: 'problem_solving',
        title: 'Problem solving',
        description: 'Understood and clarified the problem, chose a sound approach.',
        status: 'observed',
        points: 3,
        maxPoints: 4,
        evidence: [
          { kind: 'quote', text: 'Can the sentence be empty?' },
          { kind: 'run', text: 'Run 4: 8 of 8 pass' },
        ],
        feedback: 'You asked about empty input before writing code, and chose a set.',
        nextStep: 'State the time and space cost when you give your plan.',
      },
      {
        criterionId: 'code',
        title: 'Code',
        description: 'Correct on the cases, clear, idiomatic.',
        status: 'observed',
        points: 4,
        maxPoints: 4,
        evidence: [
          { kind: 'lines', text: 'solution.py, lines 2–7', startLine: 2, endLine: 7 },
          { kind: 'run', text: 'Run 4' },
        ],
        feedback: 'Clear and idiomatic: one pass, a set of the words seen so far.',
        nextStep: "Nothing to change. Keep this shape when there's a clock.",
      },
      {
        criterionId: 'testing',
        title: 'Testing',
        description: 'Examples before code, edge cases, ran the code.',
        status: 'observed',
        points: 2,
        maxPoints: 4,
        evidence: [
          { kind: 'example', text: 'Example 4: you expected "one", the problem expects "two"' },
          { kind: 'run', text: 'Run 2: 7 of 8 pass' },
        ],
        feedback: 'You wrote examples before code, but one expected the wrong answer at first.',
        nextStep: 'Trace an example word by word before you write its answer.',
      },
      {
        criterionId: 'communication',
        title: 'Communication',
        description: 'Explained the plan and decisions, answered probes.',
        status: 'insufficient_evidence',
        points: null,
        maxPoints: 4,
        evidence: [
          { kind: 'hint', text: 'Nudge at Examples' },
        ],
        feedback: "You explained your plan once; most of the session was quiet. Quiet isn't scored as a mistake.",
        nextStep: 'Say each decision out loud as you make it.',
      },
    ],
    graderModel: 'standard',
  },
  patterns: [
    {
      patternId: 'hash_set',
      name: 'Hash set lookup',
      family: 'lookup',
      recognitionCue: 'remember what you have seen in a set',
      rating: 'good',
      dueAt: 1_700_500_000_000,
    },
    {
      patternId: 'string_scan',
      name: 'String scanning',
      family: 'scanning',
      recognitionCue: 'single pass tokenization and traversal',
      rating: 'good',
      dueAt: 1_700_800_000_000,
    },
  ],
  approachSummary: 'Walk the words once, lower-casing each. Keep the words you have seen in a set; the first word already in the set is the answer.',
  targetComplexity: 'O(n) time, O(n) space',
  referenceSolution: 'def first_repeated(sentence: str) -> str | None:\n    seen = set()\n    for word in sentence.lower().split(" "):\n        if word in seen:\n            return word\n        seen.add(word)\n    return None\n',
  uncaughtMisreadings: [
    'In example 4 you answered "one", the first word that ever repeats. The problem asks for the first word to appear a second time: "two".',
  ],
  stageTimings: [
    { stage: 'understand', elapsedMs: 180000 },
    { stage: 'plan', elapsedMs: 240000 },
    { stage: 'code', elapsedMs: 480000 },
    { stage: 'verify', elapsedMs: 120000 },
  ],
  runCount: 4,
  hintsUsed: ['nudge'],
  setting: 'coached',
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

describe('Debrief', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders header, setting label, and navigation buttons', async () => {
    const onExit = vi.fn();
    const onRetryInterview = vi.fn();
    const user = userEvent.setup();

    renderWithClient(
      <Debrief
        debrief={mockDebriefDto}
        programId="program-1"
        lessonId="lesson-1"
        onExit={onExit}
        onRetryInterview={onRetryInterview}
      />,
    );

    expect(screen.getByText(/DEBRIEF · CODE · COACHED/i)).toBeInTheDocument();

    const tryMockBtn = screen.getByRole('button', { name: /Try it under interview conditions/i });
    expect(tryMockBtn).toBeInTheDocument();
    await user.click(tryMockBtn);
    expect(onRetryInterview).toHaveBeenCalledTimes(1);

    const backBtn = screen.getByRole('button', { name: /Back to Practice/i });
    expect(backBtn).toBeInTheDocument();
    await user.click(backBtn);
    expect(onExit).toHaveBeenCalledTimes(1);
  });

  it('renders objective session facts: stage timings, run count, hints used', () => {
    renderWithClient(
      <Debrief
        debrief={mockDebriefDto}
        programId="program-1"
        lessonId="lesson-1"
      />,
    );

    // What happened facts
    expect(screen.getByRole('heading', { level: 2, name: /What happened/i })).toBeInTheDocument();
    expect(screen.getByText('4 runs')).toBeInTheDocument();
    expect(screen.getByText('1 hint')).toBeInTheDocument();

    // Time per stage
    expect(screen.getByText('Time per stage')).toBeInTheDocument();
    expect(screen.getByText('Understand')).toBeInTheDocument();
    expect(screen.getByText('Plan')).toBeInTheDocument();
    expect(screen.getByText('Code')).toBeInTheDocument();
    expect(screen.getByText('Verify')).toBeInTheDocument();
  });

  it('renders the 4 scorecard criteria and shows "Not enough evidence" for insufficient evidence (never 0 of 4)', () => {
    renderWithClient(
      <Debrief
        debrief={mockDebriefDto}
        programId="program-1"
        lessonId="lesson-1"
      />,
    );

    expect(screen.getByRole('heading', { level: 2, name: /How it went/i })).toBeInTheDocument();

    // 4 criteria headings
    expect(screen.getByText('Problem solving')).toBeInTheDocument();
    expect(screen.getByText('Code')).toBeInTheDocument();
    expect(screen.getByText('Testing')).toBeInTheDocument();
    expect(screen.getByText('Communication')).toBeInTheDocument();

    // Awarded points
    expect(screen.getByText(/3 of 4|3 \/ 4/)).toBeInTheDocument();
    expect(screen.getByText(/4 of 4|4 \/ 4/)).toBeInTheDocument();
    expect(screen.getByText(/2 of 4|2 \/ 4/)).toBeInTheDocument();

    // CRITICAL: Insufficient evidence must display "Not enough evidence", never 0 of 4 or 0/4
    expect(screen.getByText('Not enough evidence')).toBeInTheDocument();
    expect(screen.queryByText(/0 of 4|0 \/ 4/)).not.toBeInTheDocument();

    // Evidence quotes and lines
    expect(screen.getByText(/Can the sentence be empty\?/i)).toBeInTheDocument();
    expect(screen.getByText(/solution\.py, lines 2–7/i)).toBeInTheDocument();

    // Feedback and next steps
    expect(screen.getByText(/State the time and space cost when you give your plan\./i)).toBeInTheDocument();
    expect(screen.getByText(/Say each decision out loud as you make it\./i)).toBeInTheDocument();
  });

  it('reveals pattern cards with rating badge, recognition cue, and next review date', () => {
    renderWithClient(
      <Debrief
        debrief={mockDebriefDto}
        programId="program-1"
        lessonId="lesson-1"
      />,
    );

    // Pattern names revealed in debrief
    expect(screen.getByText('Hash set lookup')).toBeInTheDocument();
    expect(screen.getByText('String scanning')).toBeInTheDocument();

    // Rating badges
    const goodBadges = screen.getAllByText('Good');
    expect(goodBadges.length).toBeGreaterThanOrEqual(2);

    // Recognition cues
    expect(screen.getByText(/remember what you have seen in a set/i)).toBeInTheDocument();
    expect(screen.getByText(/single pass tokenization and traversal/i)).toBeInTheDocument();

    // Coming back section
    expect(screen.getByRole('heading', { level: 2, name: /Coming back/i })).toBeInTheDocument();
  });

  it('shows strong answer with approach summary and collapsible reference solution', async () => {
    const user = userEvent.setup();

    renderWithClient(
      <Debrief
        debrief={mockDebriefDto}
        programId="program-1"
        lessonId="lesson-1"
      />,
    );

    // Approach and target complexity
    expect(screen.getByRole('heading', { level: 2, name: /The strong answer/i })).toBeInTheDocument();
    expect(screen.getByText(/O\(n\) time, O\(n\) space/i)).toBeInTheDocument();
    expect(
      screen.getByText(/Walk the words once, lower-casing each/i),
    ).toBeInTheDocument();

    // Collapsible reference code
    const toggleButton = screen.getByText(/Show the reference code/i);
    expect(toggleButton).toBeInTheDocument();
    await user.click(toggleButton);

    // Reference code should be visible
    expect(screen.getByText(/seen = set\(\)/i)).toBeInTheDocument();
  });

  it('renders SaveAsReviewCard for saving mistake or uncaught misreading', () => {
    renderWithClient(
      <Debrief
        debrief={mockDebriefDto}
        programId="program-1"
        lessonId="lesson-1"
      />,
    );

    // Save a mistake section
    expect(screen.getByRole('heading', { level: 2, name: /Save a mistake/i })).toBeInTheDocument();
    expect(screen.getByText(/In example 4 you answered "one"/i)).toBeInTheDocument();

    // Review card input and save button
    expect(screen.getByRole('button', { name: /Save as a review card/i })).toBeInTheDocument();
  });
});
