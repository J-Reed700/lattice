import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';

import type { LearningLessonDto, LearningProgramDto } from '@/lib/bindings';
import type { DueItemDto, LearningPracticeProfileDto } from './practiceHomeApi';
import type { HistoryItem } from './History';
import { type OpenPracticeTarget, PracticeHome } from './PracticeHome';

const mockProgram: LearningProgramDto = {
  summary: {
    id: 'program-1',
    title: 'Problem Solving and Interview Preparation',
    goal: 'Master algorithmic problem solving and interview communication',
    status: 'active',
    revision: 1,
    moduleCount: 1,
    lessonCount: 2,
    completedLessons: 0,
    currentLessonId: 'lesson-1',
    createdAt: 1_790_000_000_000,
  },
  priorKnowledge: '',
  minutesPerSession: 30,
  modelName: 'standard',
  modules: [],
  sources: [],
};

const mockLesson: LearningLessonDto = {
  id: 'lesson-1',
  title: 'Strings and arrays',
  preparation: 'ready',
  objective: 'Master linear scanning and two pointers',
} as LearningLessonDto;

const renderWithClient = (ui: ReactNode) => {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: Infinity },
      mutations: { retry: false },
    },
  });
  return render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
};

describe('PracticeHome', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('renders the five cards in fixed order (Explain, Spot the mistakes, Code, Mock interview, Role-play), each with one Start button', () => {
    renderWithClient(<PracticeHome program={mockProgram} currentLesson={mockLesson} />);

    const headings = screen.getAllByRole('heading', { level: 3 });
    const titles = headings.map((h) => h.textContent?.trim());
    expect(titles).toEqual([
      'Explain',
      'Spot the mistakes',
      'Code',
      'Mock interview',
      'Role-play',
    ]);

    const startButtons = screen.getAllByRole('button', { name: /^start$/i });
    expect(startButtons).toHaveLength(5);
  });

  it('shows "Fits what you\'re preparing for" badge on Mock interview when profile has interviewAt without reordering cards', () => {
    const profileWithDate: LearningPracticeProfileDto = {
      programId: 'program-1',
      details: 'Practicing algorithmic questions',
      language: 'python',
      interviewAt: Date.now() + 10 * 86_400_000,
      company: null,
      role: null,
      revision: 1,
    };

    renderWithClient(
      <PracticeHome
        program={mockProgram}
        currentLesson={mockLesson}
        profile={profileWithDate}
      />,
    );

    expect(screen.getByText("Fits what you're preparing for")).toBeInTheDocument();

    const headings = screen.getAllByRole('heading', { level: 3 });
    const titles = headings.map((h) => h.textContent?.trim());
    expect(titles).toEqual([
      'Explain',
      'Spot the mistakes',
      'Code',
      'Mock interview',
      'Role-play',
    ]);
  });

  it('shows "Fits what you\'re preparing for" badge on Mock interview when profile details mention "interview" case-insensitively', () => {
    const profileWithMention: LearningPracticeProfileDto = {
      programId: 'program-1',
      details: 'Preparing for technical INTERVIEW rounds with a live coach',
      language: 'python',
      interviewAt: null,
      company: null,
      role: null,
      revision: 1,
    };

    renderWithClient(
      <PracticeHome
        program={mockProgram}
        currentLesson={mockLesson}
        profile={profileWithMention}
      />,
    );

    expect(screen.getByText("Fits what you're preparing for")).toBeInTheDocument();

    const headings = screen.getAllByRole('heading', { level: 3 });
    const titles = headings.map((h) => h.textContent?.trim());
    expect(titles).toEqual([
      'Explain',
      'Spot the mistakes',
      'Code',
      'Mock interview',
      'Role-play',
    ]);
  });

  it('does not show the badge on Mock interview when profile has no interview date and details do not mention interview', () => {
    const profileGeneral: LearningPracticeProfileDto = {
      programId: 'program-1',
      details: 'General practice on basic data structures',
      language: 'python',
      interviewAt: null,
      company: null,
      role: null,
      revision: 1,
    };

    renderWithClient(
      <PracticeHome
        program={mockProgram}
        currentLesson={mockLesson}
        profile={profileGeneral}
      />,
    );

    expect(screen.getByRole('heading', { name: 'Mock interview' })).toBeInTheDocument();
    expect(screen.queryByText("Fits what you're preparing for")).not.toBeInTheDocument();
  });

  it('renders the Continue card when an open session, attempt, or drill exists', () => {
    const openTarget: OpenPracticeTarget = {
      id: 'session-1',
      title: 'First repeated word',
      kind: 'code',
      settingOrHelp: 'Code · Coached · 12 min in',
      stoppedAt: 'Stopped at Examples, stage 2 of 6',
      stageInfo: 'stage 2 of 6',
    };

    renderWithClient(
      <PracticeHome
        program={mockProgram}
        currentLesson={mockLesson}
        openTarget={openTarget}
      />,
    );

    expect(screen.getByText('Continue')).toBeInTheDocument();
    expect(screen.getByText('First repeated word')).toBeInTheDocument();
    expect(screen.getByText('Stopped at Examples, stage 2 of 6')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^continue$/i })).toBeInTheDocument();
  });

  it('does not render the Continue card when no open target exists', () => {
    renderWithClient(
      <PracticeHome
        program={mockProgram}
        currentLesson={mockLesson}
        openTarget={null}
      />,
    );

    expect(screen.getByRole('heading', { name: 'Code' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^continue$/i })).not.toBeInTheDocument();
  });

  it('renders DueStrip and History with filters', () => {
    const dueItems: DueItemDto[] = [
      {
        id: 'due-1',
        kind: 'pattern',
        title: '3 problem patterns',
        subtitle: '1 overdue by 2 days, 2 due today',
        isOverdue: true,
      },
    ];

    const historyItems: HistoryItem[] = [
      {
        id: 'hist-1',
        when: 'Yesterday',
        what: 'Count distinct words',
        kind: 'code',
        settingOrHelp: 'Coached',
        result: 'All 8 cases pass',
      },
    ];

    renderWithClient(
      <PracticeHome
        program={mockProgram}
        currentLesson={mockLesson}
        dueItems={dueItems}
        historyItems={historyItems}
      />,
    );

    expect(screen.getByRole('heading', { name: /due/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /start the most overdue/i })).toBeInTheDocument();

    expect(screen.getByRole('heading', { name: /history/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^all$/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^explain$/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^spot the mistakes$/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^code$/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^mock interview$/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^role-play$/i })).toBeInTheDocument();
  });

  it('ensures Mock interview offers Mock and Watched settings only, while Coached stays on Code', () => {
    renderWithClient(<PracticeHome program={mockProgram} currentLesson={mockLesson} />);

    const mockCard = screen.getByTestId('card-mock');
    expect(within(mockCard).queryByText(/coached/i)).not.toBeInTheDocument();
    expect(within(mockCard).getByText(/watched|mock/i)).toBeInTheDocument();

    const codeCard = screen.getByTestId('card-code');
    expect(within(codeCard).getByText(/coached/i)).toBeInTheDocument();
  });

  it('adapts default practice modes to non-coding courses (e.g. a biology course on mitochondria)', () => {
    const biologyProgram: LearningProgramDto = {
      ...mockProgram,
      summary: {
        ...mockProgram.summary,
        id: 'program-bio-1',
        title: 'Cellular Biology',
        goal: 'Understand organelle structure and how mitochondria generate ATP',
      },
    };
    const mitochondriaLesson: LearningLessonDto = {
      id: 'lesson-mito',
      title: 'Mitochondria and cellular respiration',
      preparation: 'ready',
      objective: 'Explain the electron transport chain and ATP synthesis',
    } as LearningLessonDto;

    renderWithClient(
      <PracticeHome program={biologyProgram} currentLesson={mitochondriaLesson} />,
    );

    const spotCard = screen.getByTestId('card-spot');
    expect(within(spotCard).getByText('explanation')).toBeInTheDocument();

    const mockCard = screen.getByTestId('card-mock');
    expect(within(mockCard).getByText('technical questions')).toBeInTheDocument();
  });
});
