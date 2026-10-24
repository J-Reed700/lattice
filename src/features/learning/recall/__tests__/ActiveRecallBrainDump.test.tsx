import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

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
    {
      kind: 'reflection',
      title: 'What resets follower election timers?',
      body: 'Leader heartbeats',
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

  afterEach(() => {
    vi.useRealTimers();
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

  it('toggles self-test timer and ticks forward', () => {
    vi.useFakeTimers();
    renderWithClient(
      <ActiveRecallBrainDump program={mockProgram} lesson={mockLesson} />,
    );

    const timerButton = screen.getByRole('button', { name: /Start timer/i });
    expect(screen.getByText('00:00')).toBeInTheDocument();

    fireEvent.click(timerButton);
    expect(screen.getByRole('button', { name: /Pause timer/i })).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(screen.getByText('00:05')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /Pause timer/i }));
    act(() => {
      vi.advanceTimersByTime(3000);
    });
    expect(screen.getByText('00:05')).toBeInTheDocument();
    vi.useRealTimers();
  });

  it('falls back gracefully to program-level checkpoints when lesson is omitted', async () => {
    const user = userEvent.setup();
    renderWithClient(<ActiveRecallBrainDump program={mockProgram} />);

    expect(screen.getByText(/Persisted locally for this program/i)).toBeInTheDocument();

    const textarea = screen.getByRole('textbox', {
      name: /Your free-recall memory dump/i,
    });
    await user.type(textarea, 'Distributed consensus algorithms ensure leader election and replication.');

    const compareButton = screen.getByRole('button', {
      name: /Compare with Lesson & Check Recall/i,
    });
    await user.click(compareButton);

    expect(screen.getAllByText('Program Goal').length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText(/Recall Coverage/i)).toBeInTheDocument();
  });

  it('allows toggling between revealed comparison and continuing writing dump', async () => {
    const user = userEvent.setup();
    renderWithClient(
      <ActiveRecallBrainDump program={mockProgram} lesson={mockLesson} />,
    );

    const textarea = screen.getByRole('textbox', {
      name: /Your free-recall memory dump/i,
    });
    await user.type(textarea, 'Heartbeat timers reset follower timeouts.');

    const compareButton = screen.getByRole('button', {
      name: /Compare with Lesson & Check Recall/i,
    });
    await user.click(compareButton);
    expect(screen.getByText(/Recall Coverage/i)).toBeInTheDocument();

    const continueBtn = screen.getByRole('button', { name: /Continue writing dump/i });
    await user.click(continueBtn);

    expect(screen.getByRole('textbox', {
      name: /Your free-recall memory dump/i,
    })).toBeInTheDocument();
  });

  it('prompts confirmation on clear and resets dump only when confirmed', async () => {
    const user = userEvent.setup();
    const confirmSpy = vi.spyOn(window, 'confirm');

    renderWithClient(
      <ActiveRecallBrainDump program={mockProgram} lesson={mockLesson} />,
    );

    const textarea = screen.getByRole('textbox', {
      name: /Your free-recall memory dump/i,
    });
    await user.type(textarea, 'Some text to be cleared.');

    const clearButton = screen.getByRole('button', { name: 'Clear' });

    // Cancelled confirmation
    confirmSpy.mockReturnValueOnce(false);
    await user.click(clearButton);
    expect(screen.getByDisplayValue('Some text to be cleared.')).toBeInTheDocument();

    // Accepted confirmation
    confirmSpy.mockReturnValueOnce(true);
    await user.click(clearButton);
    expect(screen.queryByDisplayValue('Some text to be cleared.')).not.toBeInTheDocument();
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

    vi.useFakeTimers();
    fireEvent.click(draftButtons[0]);
    expect(onDraftMissedConcept).toHaveBeenCalled();
    act(() => {
      vi.advanceTimersByTime(2600);
    });
    vi.useRealTimers();
  });

  it('achieves 100% recall coverage when all concepts are remembered', async () => {
    const user = userEvent.setup();
    renderWithClient(
      <ActiveRecallBrainDump program={mockProgram} lesson={mockLesson} />
    );

    const textarea = screen.getByRole('textbox', {
      name: /Your free-recall memory dump/i,
    });
    await user.type(
      textarea,
      'Understand heartbeat timeouts, election terms, and split-vote resolution. Heartbeat timers reset follower timeouts upon leader heartbeats. Split vote handling minimizes split votes. What resets follower election timers?',
    );

    const compareButton = screen.getByRole('button', {
      name: /Compare with Lesson & Check Recall/i,
    });
    await user.click(compareButton);

    expect(screen.getByText(/4 of 4 key concepts recalled \(100%\)/i)).toBeInTheDocument();
  });

  it('handles lesson with empty title, objective, and block title', async () => {
    const user = userEvent.setup();
    const minimalLesson: LearningLessonDto = {
      id: 'min-lesson',
      title: '',
      objective: '',
      estimatedMinutes: 5,
      preparation: 'ready',
      blocks: [{ kind: 'explanation', title: '', body: 'Basic concept block', sourceIds: [] }],
      completed: false,
    };

    renderWithClient(
      <ActiveRecallBrainDump program={mockProgram} lesson={minimalLesson} />
    );

    const textarea = screen.getByRole('textbox', {
      name: /Your free-recall memory dump/i,
    });
    await user.type(textarea, 'Basic concept block and short question answer.');
    await user.click(screen.getByRole('button', { name: /Compare with Lesson & Check Recall/i }));

    expect(screen.getByText('Block 1')).toBeInTheDocument();
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

  it('renders correctly when cards array is empty or contains a single card', () => {
    const { rerender } = render(<SpacedRepetitionCurve cards={[]} />);

    expect(screen.getByText('0 total cards')).toBeInTheDocument();
    expect(screen.getByText(/No recall cards currently scheduled/i)).toBeInTheDocument();

    rerender(<SpacedRepetitionCurve cards={[sampleCards[0]]} />);
    expect(screen.getByText('1 total card')).toBeInTheDocument();
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
      {
        id: 'card-1-essay',
        format: 'question_answer',
        deckId: 'deck-1',
        topic: 'OS Concepts',
        question: 'Explain thrashing.',
        answer: 'Thrashing occurs when memory pages constantly fault.',
        options: [],
        correctIndex: 0,
        explanation: 'Virtual memory overcommit causes thrashing.',
        source: { chunkId: '1b', documentId: '1', fileName: 'os.md', filePath: '/os.md', excerpt: 'Thrashing' },
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
        topic: '',
        question: 'What is a memtable?',
        answer: 'An in-memory write buffer.',
        options: ['An in-memory write buffer', 'On-disk SSTable', 'Commit log'],
        correctIndex: 0,
        explanation: 'Writes enter memtable before flushing to SSTable.',
        source: { chunkId: '2', documentId: '2', fileName: 'db.md', filePath: '/db.md', excerpt: 'Memtable' },
        dueAt: 0,
        intervalDays: 1,
        reviewCount: 1,
        lapses: 1,
      },
      {
        id: 'card-2-future',
        format: 'multiple_choice',
        deckId: 'deck-2',
        topic: 'LSM Trees',
        question: 'What is write amplification?',
        answer: 'Ratio of bytes written to storage vs application writes.',
        options: ['Ratio of writes', 'Network traffic', 'Memory ratio'],
        correctIndex: 0,
        explanation: 'Compaction causes write amplification.',
        source: { chunkId: '2b', documentId: '2', fileName: 'db.md', filePath: '/db.md', excerpt: 'Write amp' },
        dueAt: Date.now() + 10_000_000,
        intervalDays: 10,
        reviewCount: 3,
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

  it('starts interleaved session and displays Interleaved Topic Mix badge and exits session', async () => {
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

    // Exit session
    const exitBtn = screen.getByRole('button', { name: 'Exit session' });
    await user.click(exitBtn);

    expect(screen.getByText(/Interleaved Multi-Deck Study Session/i)).toBeInTheDocument();
  });

  it('supports selecting and deselecting decks, mix all, and clear selection', async () => {
    const user = userEvent.setup();
    renderWithClient(<InterleavedStudyPanel decks={sampleDecks} />);

    await user.click(screen.getByRole('button', { name: /Open interleaved mixer/i }));

    // Clear selection
    const clearBtn = screen.getByRole('button', { name: 'Clear' });
    await user.click(clearBtn);

    expect(screen.getByText(/Select 2 or more decks to enable interleaved practice/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Start Interleaved Session/i })).toBeDisabled();

    // Mix all decks
    const mixAllBtn = screen.getByRole('button', { name: 'Mix all decks' });
    await user.click(mixAllBtn);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Start Interleaved Session/i })).toBeEnabled();
    });

    // Toggle individual deck checkbox
    const deckCheckboxes = screen.getAllByRole('checkbox');
    // First checkbox in options is "Due & weak cards only", deck checkboxes follow
    const osCheckbox = deckCheckboxes.find((cb) => cb.closest('label')?.textContent?.includes('Operating Systems'));
    expect(osCheckbox).toBeDefined();
    await user.click(osCheckbox!);

    // Now only 1 deck is selected -> disabled
    expect(screen.getByText(/Select 2 or more decks to enable interleaved practice/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Start Interleaved Session/i })).toBeDisabled();
  });

  it('switches between flashcard and practice quiz modes and toggles due filter', async () => {
    const user = userEvent.setup();
    renderWithClient(<InterleavedStudyPanel decks={sampleDecks} />);

    await user.click(screen.getByRole('button', { name: /Open interleaved mixer/i }));

    const quizTab = screen.getByRole('button', { name: 'Practice Quiz' });
    await user.click(quizTab);

    const flashcardTab = screen.getByRole('button', { name: 'Flashcards' });
    await user.click(flashcardTab);

    const dueFilterCheckbox = screen.getByLabelText(/Due & weak cards only/i);
    expect(dueFilterCheckbox).not.toBeChecked();
    await user.click(dueFilterCheckbox);
    expect(dueFilterCheckbox).toBeChecked();
  });

  it('filters candidate cards and launches practice quiz session', async () => {
    const user = userEvent.setup();
    renderWithClient(<InterleavedStudyPanel decks={sampleDecks} />);

    await user.click(screen.getByRole('button', { name: /Open interleaved mixer/i }));

    // Switch to Practice Quiz mode
    await user.click(screen.getByRole('button', { name: 'Practice Quiz' }));

    // Enable due & weak cards only
    await user.click(screen.getByLabelText(/Due & weak cards only/i));

    const startButton = await screen.findByRole('button', {
      name: /Start Interleaved Session/i,
    });
    expect(startButton).toBeEnabled();
    await user.click(startButton);

    expect(screen.getByText(/Interleaved Topic Mix/i)).toBeInTheDocument();
  });
});
