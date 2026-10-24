import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ActiveRecallBrainDump } from '@/features/learning/recall/ActiveRecallBrainDump';
import { InterleavedStudyPanel } from '@/features/learning/recall/flashcards/InterleavedStudyPanel';
import { SpacedRepetitionCurve } from '@/features/learning/recall/SpacedRepetitionCurve';
import type {
  LearningLessonDto,
  LearningProgramDto,
  StudyDeckDto,
  StudyDeckSummaryDto,
} from '@/lib/bindings';

const mocks = vi.hoisted(() => ({
  getStudyDeck: vi.fn(),
  reviewStudyCard: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  default: {
    getStudyDeck: mocks.getStudyDeck,
    reviewStudyCard: mocks.reviewStudyCard,
  },
}));

const mockProgram: LearningProgramDto = {
  summary: {
    id: 'program-1',
    title: 'Distributed Systems',
    goal: 'Master consensus algorithms, fault tolerance, and replication.',
    status: 'active',
    revision: 1,
    moduleCount: 1,
    lessonCount: 1,
    completedLessons: 0,
    currentLessonId: 'lesson-1',
    createdAt: 1_700_000_000_000,
  },
  priorKnowledge: 'Basic networking',
  minutesPerSession: 25,
  modelName: 'gpt-4',
  modules: [
    {
      id: 'module-1',
      title: 'Consensus Protocols',
      summary: 'Learn Paxos and Raft mechanisms.',
      outcomes: ['Differentiate leader election from log replication'],
      lessons: [],
    },
  ],
  sources: [],
  attempts: [],
};

const mockLesson: LearningLessonDto = {
  id: 'lesson-1',
  title: 'Raft Leader Election',
  objective: 'Understand heartbeat timeouts, election terms, and split-vote resolution.',
  estimatedMinutes: 20,
  preparation: 'ready',
  blocks: [
    {
      kind: 'explanation',
      title: 'Heartbeat Timers',
      body: 'Followers reset their randomized timeout upon receiving valid heartbeats from the leader.',
      sourceIds: [],
    },
    {
      kind: 'worked_example',
      title: 'Split Vote Handling',
      body: 'Randomized election timeouts minimize the probability of split votes between candidates.',
      sourceIds: [],
    },
  ],
  questions: [
    {
      id: 'q1',
      kind: 'quiz',
      prompt: 'What resets follower election timers?',
      options: ['Leader heartbeats', 'Client queries'],
      sourceIds: [],
    },
  ],
  completed: false,
};

function renderWithClient(ui: React.ReactNode) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  return render(
    <QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>,
  );
}

describe('ActiveRecallBrainDump', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
  });

  it('renders evidence explanation and notes-closed lock initially', () => {
    renderWithClient(
      <ActiveRecallBrainDump program={mockProgram} lesson={mockLesson} />,
    );

    expect(
      screen.getByText(/Close your notes and write down or speak everything you remember from scratch/i),
    ).toBeInTheDocument();
    expect(screen.getByText(/Notes-closed lock active/i)).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: /Compare with Lesson & Check Recall/i }),
    ).toBeDisabled();
    expect(screen.queryByText(/Recall Coverage/i)).not.toBeInTheDocument();
  });

  it('updates word count as learner types and persists to localStorage', async () => {
    const user = userEvent.setup();
    renderWithClient(
      <ActiveRecallBrainDump program={mockProgram} lesson={mockLesson} />,
    );

    const textarea = screen.getByRole('textbox', {
      name: /Your free-recall memory dump/i,
    });
    await user.type(textarea, 'Raft leader sends randomized heartbeats to followers.');

    expect(screen.getByText('7 words')).toBeInTheDocument();
    expect(
      localStorage.getItem('active-recall-dump:program-1:lesson-1'),
    ).toBe('Raft leader sends randomized heartbeats to followers.');
  });

  it('toggles self-test timer', async () => {
    const user = userEvent.setup();
    renderWithClient(
      <ActiveRecallBrainDump program={mockProgram} lesson={mockLesson} />,
    );

    const timerButton = screen.getByRole('button', { name: /Start timer/i });
    expect(screen.getByText('00:00')).toBeInTheDocument();

    await user.click(timerButton);
    expect(screen.getByRole('button', { name: /Pause timer/i })).toBeInTheDocument();
  });

  it('compares free recall against lesson checkpoints and highlights covered vs gaps', async () => {
    const user = userEvent.setup();
    const onDraftMissedConcept = vi.fn();

    renderWithClient(
      <ActiveRecallBrainDump
        program={mockProgram}
        lesson={mockLesson}
        onDraftMissedConcept={onDraftMissedConcept}
      />
    );

    const textarea = screen.getByRole('textbox', {
      name: /Your free-recall memory dump/i,
    });
    // Write text that covers heartbeats and timeouts, but omits split votes
    await user.type(
      textarea,
      'The leader uses heartbeats and randomized timeout to maintain consensus leadership.',
    );

    const compareButton = screen.getByRole('button', {
      name: /Compare with Lesson & Check Recall/i,
    });
    expect(compareButton).toBeEnabled();
    await user.click(compareButton);

    expect(screen.getByText(/Recall Coverage/i)).toBeInTheDocument();
    expect(screen.getByText(/Recalled Concepts/i)).toBeInTheDocument();
    expect(screen.getByText(/Gaps to Revisit/i)).toBeInTheDocument();

    // Check for "Turn missed concept into recall card" action
    const draftButtons = screen.getAllByRole('button', {
      name: /Turn missed concept into recall card/i,
    });
    expect(draftButtons.length).toBeGreaterThan(0);

    await user.click(draftButtons[0]);
    expect(onDraftMissedConcept).toHaveBeenCalled();
  });
});

describe('SpacedRepetitionCurve', () => {
  const sampleCards = [
    { id: 'c1', dueAt: Date.now() - 1000, intervalDays: 0.5, reviewCount: 1 },
    { id: 'c2', dueAt: Date.now() + 86400000, intervalDays: 2, reviewCount: 2 },
    { id: 'c3', dueAt: Date.now() + 86400000 * 4, intervalDays: 5, reviewCount: 3 },
    { id: 'c4', dueAt: Date.now() + 86400000 * 8, intervalDays: 10, reviewCount: 4 },
    { id: 'c5', dueAt: Date.now() + 86400000 * 20, intervalDays: 25, reviewCount: 5, stability: 25 },
  ];

  it('renders Ebbinghaus forgetting curve explanation and live card distribution tiers', () => {
    render(<SpacedRepetitionCurve cards={sampleCards} />);

    expect(
      screen.getByText(/Combat the forgetting curve \(where most new information is lost within 48 hours\)/i),
    ).toBeInTheDocument();

    expect(screen.getByText('Initial (< 1 day)')).toBeInTheDocument();
    expect(screen.getByText('1–2 days')).toBeInTheDocument();
    expect(screen.getByText('3–6 days')).toBeInTheDocument();
    expect(screen.getByText('1 week (7–13 days)')).toBeInTheDocument();
    expect(screen.getByText('2+ weeks (14+ days)')).toBeInTheDocument();

    expect(screen.getByText('1 mastered (14d+)')).toBeInTheDocument();
  });

  it('renders SVG chart with forgetting curve markers', () => {
    render(<SpacedRepetitionCurve cards={sampleCards} />);

    expect(screen.getByText('Day 1')).toBeInTheDocument();
    expect(screen.getByText('Day 3')).toBeInTheDocument();
    expect(screen.getByText('Day 7 (1 wk)')).toBeInTheDocument();
    expect(screen.getByText('Day 14 (2 wks)')).toBeInTheDocument();
    expect(screen.getByText('Day 30 (1 mo)')).toBeInTheDocument();
    expect(screen.getByText(/48h drop/i)).toBeInTheDocument();
  });
});

describe('InterleavedStudyPanel', () => {
  const sampleDecks: StudyDeckSummaryDto[] = [
    {
      id: 'deck-1',
      title: 'Operating Systems',
      focus: 'Virtual memory',
      cardCount: 5,
      dueCount: 2,
      quizAttempts: 10,
      quizCorrect: 8,
      createdAt: 100,
    },
    {
      id: 'deck-2',
      title: 'Database Engines',
      focus: 'LSM trees',
      cardCount: 6,
      dueCount: 3,
      quizAttempts: 5,
      quizCorrect: 4,
      createdAt: 100,
    },
  ];

  const fullDeck1: StudyDeckDto = {
    id: 'deck-1',
    title: 'Operating Systems',
    focus: 'Virtual memory',
    modelName: 'gpt-4',
    createdAt: 100,
    cards: [
      {
        id: 'card-1',
        format: 'multiple_choice',
        deckId: 'deck-1',
        topic: 'Virtual Memory',
        question: 'What is a page fault?',
        answer: 'Accessing memory not currently mapped in RAM.',
        options: ['Hardware interrupt', 'Accessing memory not mapped in RAM', 'Cache hit'],
        correctIndex: 1,
        explanation: 'The OS page table swaps pages on page faults.',
        source: { chunkId: '1', documentId: '1', fileName: 'os.md', filePath: '/os.md', excerpt: 'Page fault' },
        dueAt: 0,
        intervalDays: 1,
        reviewCount: 1,
        lapses: 0,
      },
    ],
  };

  const fullDeck2: StudyDeckDto = {
    id: 'deck-2',
    title: 'Database Engines',
    focus: 'LSM trees',
    modelName: 'gpt-4',
    createdAt: 100,
    cards: [
      {
        id: 'card-2',
        format: 'multiple_choice',
        deckId: 'deck-2',
        topic: 'LSM Trees',
        question: 'What is a memtable?',
        answer: 'An in-memory write buffer.',
        options: ['An in-memory write buffer', 'On-disk SSTable', 'Commit log'],
        correctIndex: 0,
        explanation: 'Writes enter memtable before flushing to SSTable.',
        source: { chunkId: '2', documentId: '2', fileName: 'db.md', filePath: '/db.md', excerpt: 'Memtable' },
        dueAt: 0,
        intervalDays: 1,
        reviewCount: 1,
        lapses: 0,
      },
    ],
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getStudyDeck.mockImplementation((id: string) => {
      if (id === 'deck-1') return Promise.resolve({ ok: true, data: fullDeck1 });
      if (id === 'deck-2') return Promise.resolve({ ok: true, data: fullDeck2 });
      return Promise.resolve({ ok: false, error: 'Deck not found' });
    });
  });

  it('explains interleaving and lists decks with selection controls', async () => {
    const user = userEvent.setup();
    renderWithClient(<InterleavedStudyPanel decks={sampleDecks} />);

    const openButton = screen.getByRole('button', { name: /Open interleaved mixer/i });
    await user.click(openButton);

    expect(
      screen.getByText(/Mix up different sub-topics, decks, or problem types during a single study session/i),
    ).toBeInTheDocument();

    expect(screen.getByText('Operating Systems')).toBeInTheDocument();
    expect(screen.getByText('Database Engines')).toBeInTheDocument();

    await waitFor(() => {
      expect(
        screen.getByRole('button', { name: /Start Interleaved Session/i }),
      ).toBeEnabled();
    });
  });

  it('starts interleaved session and displays Interleaved Topic Mix badge', async () => {
    const user = userEvent.setup();
    renderWithClient(<InterleavedStudyPanel decks={sampleDecks} />);

    const openButton = screen.getByRole('button', { name: /Open interleaved mixer/i });
    await user.click(openButton);

    const startButton = await screen.findByRole('button', {
      name: /Start Interleaved Session/i,
    });
    await user.click(startButton);

    expect(screen.getByText(/Interleaved Topic Mix/i)).toBeInTheDocument();
    expect(screen.getByText('What is a page fault?')).toBeInTheDocument();
  });
});
