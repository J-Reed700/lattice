import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { learningApi } from '@/features/learning/api/client';
import { learningTangentsKey } from '@/features/learning/api/learningQueryKeys';
import { LessonCard } from '@/features/learning/lessons/LessonCard';
import { CourseTangentsSection } from '@/features/learning/tangents/CourseTangentsSection';
import {
  ProgramTangents,
  ProgramTangentsTrigger,
} from '@/features/learning/tangents/ProgramTangents';
import type { LearningProgramDto, LearningTangentDto } from '@/lib/bindings';

const mockNavigate = vi.fn();
const mockSelectConversation = vi.fn().mockResolvedValue(undefined);
const mockSendMessage = vi.fn().mockResolvedValue('answered');
const mockCancelGeneration = vi.fn().mockResolvedValue(undefined);
const mockDeleteConversation = vi.fn().mockResolvedValue(true);
const mockSetSelectedSpace = vi.fn();

vi.mock('react-router', async () => {
  const actual = await vi.importActual<typeof import('react-router')>('react-router');
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  };
});

vi.mock('@/shared/conversations/conversationsStore', async () => {
  const actual = await vi.importActual<typeof import('@/shared/conversations/conversationsStore')>(
    '@/shared/conversations/conversationsStore',
  );
  return {
    ...actual,
    useConversationsStore: (selector?: (_state: unknown) => unknown) => {
      const state = {
        spaces: [],
        selectedSpaceId: null,
        conversations: [],
        inFlightGenerations: new Map(),
        optimisticMessages: new Map(),
        messageBookmarkMap: new Map(),
        messageBookmarks: [],
        lastMessageSources: new Map(),
        messageVerification: new Map(),
        messageRetrieval: new Map(),
        messageTurn: new Map(),
        liveRetrieval: new Map(),
        liveSteps: new Map(),
        composerDraft: null,
        selectConversation: mockSelectConversation,
        sendMessage: mockSendMessage,
        cancelGeneration: mockCancelGeneration,
        deleteConversation: mockDeleteConversation,
        setSelectedSpace: mockSetSelectedSpace,
        regenerateResponse: vi.fn(),
      };
      return selector ? selector(state) : state;
    },
  };
});

vi.mock('@/features/learning/api/client', () => ({
  learningApi: {
    listLearningTangents: vi.fn(),
    createLearningTangent: vi.fn(),
    // The lesson card reads the plan's jobs and the lesson's claim review.
    getLearningPlan: vi.fn(async () => ({ ok: false, error: 'Not used in this test.' })),
    getLearningLessonClaimReview: vi.fn(async () => ({ ok: true, data: null })),
  },
}));

vi.mock('@/features/chat/api/conversationQueryData', () => ({
  fetchMessages: vi.fn().mockResolvedValue([
    {
      id: 'seed-msg',
      conversationId: 'tangent-conv-1',
      role: 'user',
      content: 'Seed message',
      status: 'completed',
      createdAt: 1000,
    },
    {
      id: 'reply-msg-1',
      conversationId: 'tangent-conv-1',
      role: 'assistant',
      content: 'Here is an explanation of randomized timers.',
      status: 'completed',
      createdAt: 1010,
    },
  ]),
  fetchConversationDetail: vi.fn().mockResolvedValue({
    id: 'tangent-conv-1',
    title: 'Leader Election',
    spaceId: 'space-1',
    messages: [],
  }),
  fetchBookmarks: vi.fn().mockResolvedValue([]),
  unwrap: (r: { ok: boolean; data?: unknown }) => r.data,
}));

const mockProgram: LearningProgramDto = {
  summary: {
    id: 'test-program',
    title: 'Distributed Systems',
    goal: 'Master consensus algorithms',
    status: 'active',
    revision: 1,
    moduleCount: 1,
    lessonCount: 1,
    completedLessons: 0,
    currentLessonId: 'lesson-1',
    createdAt: 1000,
  },
  priorKnowledge: 'None',
  minutesPerSession: 30,
  modelName: 'test-model',
  outlineReview: null,
  modules: [
    {
      id: 'module-1',
      title: 'Consensus Basics',
      summary: 'Learn Paxos and Raft',
      outcomes: ['Understand Raft leader election'],
      prerequisiteModuleIds: [],
      project: null,
      lessons: [
        {
          id: 'lesson-1',
          title: 'Raft Consensus',
          objective: 'Understand leader election and log replication',
          estimatedMinutes: 20,
          preparation: 'ready',
          completed: false,
          blocks: [
            {
              kind: 'explanation',
              title: 'Leader Election',
              body: 'In Raft, leaders are elected through randomized timers and majority votes.',
              sourceIds: [],
            },
            {
              kind: 'worked_example',
              title: 'Log Replication Walkthrough',
              body: 'Entries are appended to follower logs once quorum acknowledges.',
              sourceIds: [],
            },
          ],
        },
      ],
    },
  ],
  sources: [],
};

const mockTangent: LearningTangentDto = {
  conversationId: 'tangent-conv-1',
  programId: 'test-program',
  programTitle: 'Distributed Systems',
  spaceId: 'space-1',
  moduleId: 'module-1',
  moduleTitle: 'Consensus Basics',
  lessonId: 'lesson-1',
  lessonTitle: 'Raft Consensus',
  sectionIndex: 0,
  sectionTitle: 'Leader Election',
  selectedText: 'In Raft, leaders are elected through randomized timers and majority votes.',
  title: 'Leader Election',
  createdAt: 1000,
  updatedAt: new Date(1000).toISOString(),
  messageCount: 3,
  contextMessageCount: 1,
};

const mockTangent2: LearningTangentDto = {
  conversationId: 'tangent-conv-2',
  programId: 'program-2',
  programTitle: 'Compiler Design',
  spaceId: 'space-2',
  moduleId: 'module-2',
  moduleTitle: 'Lexing & Parsing',
  lessonId: 'lesson-2',
  lessonTitle: 'AST Generation',
  sectionIndex: 1,
  sectionTitle: 'Grammar Ambiguity',
  selectedText: 'Shift-reduce conflicts occur when grammar is ambiguous.',
  title: 'Grammar Ambiguity',
  createdAt: 2000,
  updatedAt: new Date(2000).toISOString(),
  messageCount: 2,
  contextMessageCount: 1,
};

function createTestWrapper(client: QueryClient) {
  return function TestWrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={client}>
        <TooltipProvider>{children}</TooltipProvider>
      </QueryClientProvider>
    );
  };
}

function selectText(start: Node, end: Node = start) {
  const range = document.createRange();
  range.setStart(start, 0);
  range.setEnd(end, end.textContent?.length ?? 0);
  window.getSelection()?.removeAllRanges();
  window.getSelection()?.addRange(range);
}

describe('ProgramTangents and Study Guide Tangents integration', () => {
  let queryClient: QueryClient;

  beforeEach(() => {
    vi.clearAllMocks();
    window.getSelection()?.removeAllRanges();
    queryClient = new QueryClient({
      defaultOptions: {
        queries: { retry: false, staleTime: Infinity },
        mutations: { retry: false },
      },
    });
    vi.mocked(learningApi.listLearningTangents).mockResolvedValue([mockTangent]);
    vi.mocked(learningApi.createLearningTangent).mockResolvedValue(mockTangent);
    queryClient.setQueryData(learningTangentsKey('test-program'), [mockTangent]);
  });

  it('renders LessonCard inside ProgramTangents showing end-of-section tangent buttons and toolbar trigger', async () => {
    const Wrapper = createTestWrapper(queryClient);

    render(
      <Wrapper>
        <ProgramTangents
          program={mockProgram}
          currentModuleId="module-1"
          currentLessonId="lesson-1"
        >
          <header>
            <ProgramTangentsTrigger />
          </header>
          <LessonCard
            lesson={mockProgram.modules[0].lessons[0]}
            moduleId="module-1"
            program={mockProgram}
            isCurrent
            selected
            pending={false}
            onPrepare={vi.fn()}
            onComplete={vi.fn()}
            onSelect={vi.fn()}
            onPractice={vi.fn()}
          />
        </ProgramTangents>
      </Wrapper>,
    );

    // Toolbar trigger
    const trigger = screen.getByRole('button', { name: 'Course tangents' });
    expect(trigger).toBeInTheDocument();
    expect(trigger).toHaveTextContent('Tangents');
    expect(trigger).toHaveTextContent('1');

    // Section 0 Start tangent button
    const section0Btn = screen.getByTestId('section-tangent-btn-0');
    expect(section0Btn).toBeInTheDocument();
    expect(section0Btn).toHaveTextContent('Start tangent');

    // Section 1 Start tangent button
    const section1Btn = screen.getByTestId('section-tangent-btn-1');
    expect(section1Btn).toBeInTheDocument();

    // Lesson-level Start tangent button
    const lessonBtn = screen.getByTestId('lesson-tangent-btn');
    expect(lessonBtn).toBeInTheDocument();
    expect(lessonBtn).toHaveTextContent('Start a tangent on this lesson');
  });

  it('clicking an end-of-section Start tangent button calls createLearningTangent and opens docked tangent drawer', async () => {
    const Wrapper = createTestWrapper(queryClient);

    render(
      <Wrapper>
        <ProgramTangents
          program={mockProgram}
          currentModuleId="module-1"
          currentLessonId="lesson-1"
        >
          <LessonCard
            lesson={mockProgram.modules[0].lessons[0]}
            moduleId="module-1"
            program={mockProgram}
            isCurrent
            selected
            pending={false}
            onPrepare={vi.fn()}
            onComplete={vi.fn()}
            onSelect={vi.fn()}
            onPractice={vi.fn()}
          />
        </ProgramTangents>
      </Wrapper>,
    );

    const section0Btn = screen.getByTestId('section-tangent-btn-0');
    fireEvent.click(section0Btn);

    await waitFor(() => {
      expect(learningApi.createLearningTangent).toHaveBeenCalledWith({
        programId: 'test-program',
        moduleId: 'module-1',
        lessonId: 'lesson-1',
        sectionIndex: 0,
        sectionTitle: 'Leader Election',
        selectedText: expect.stringContaining('In Raft, leaders are elected'),
      });
    });

    // Docked tangent chat drawer opens with the quote
    await waitFor(() => {
      const drawer = screen.getByRole('complementary', { name: 'Course tangents' });
      expect(drawer).toBeInTheDocument();
      expect(
        within(drawer).getByText(
          'In Raft, leaders are elected through randomized timers and majority votes.',
        ),
      ).toBeInTheDocument();
    });
  });

  it('selecting text inside a lesson section shows floating Ask in a tangent button and creates tangent', async () => {
    const Wrapper = createTestWrapper(queryClient);

    render(
      <Wrapper>
        <ProgramTangents
          program={mockProgram}
          currentModuleId="module-1"
          currentLessonId="lesson-1"
        >
          <LessonCard
            lesson={mockProgram.modules[0].lessons[0]}
            moduleId="module-1"
            program={mockProgram}
            isCurrent
            selected
            pending={false}
            onPrepare={vi.fn()}
            onComplete={vi.fn()}
            onSelect={vi.fn()}
            onPractice={vi.fn()}
          />
        </ProgramTangents>
      </Wrapper>,
    );

    const bodyParagraph = screen.getByText(/randomized timers and majority votes/i);
    selectText(bodyParagraph.firstChild!);
    fireEvent.contextMenu(bodyParagraph, { clientX: 100, clientY: 100 });

    const askButton = screen.getByRole('menuitem', { name: 'Ask in a tangent' });
    expect(askButton).toBeInTheDocument();

    fireEvent.click(askButton);

    await waitFor(() => {
      expect(learningApi.createLearningTangent).toHaveBeenCalledWith(
        expect.objectContaining({
          programId: 'test-program',
          moduleId: 'module-1',
          lessonId: 'lesson-1',
          sectionIndex: 0,
          sectionTitle: 'Leader Election',
        }),
      );
    });
  });

  it('opening the Course Tangents drawer lists tangents organized by course/module/lesson with section badges', async () => {
    const Wrapper = createTestWrapper(queryClient);

    render(
      <Wrapper>
        <ProgramTangents
          program={mockProgram}
          currentModuleId="module-1"
          currentLessonId="lesson-1"
        >
          <header>
            <ProgramTangentsTrigger />
          </header>
          <div>Workspace Content</div>
        </ProgramTangents>
      </Wrapper>,
    );

    const trigger = screen.getByRole('button', { name: 'Course tangents' });
    fireEvent.click(trigger);

    expect(screen.getByRole('complementary', { name: 'Course tangents' })).toBeInTheDocument();
    expect(screen.getByText('Course Tangents')).toBeInTheDocument();
    expect(screen.getByText(/This Lesson \(1\)/)).toBeInTheDocument();
    expect(screen.getByText(/All in Course \(1\)/)).toBeInTheDocument();

    // Module & Lesson grouping
    expect(screen.getByText('Consensus Basics')).toBeInTheDocument();
    expect(screen.getByText('Raft Consensus')).toBeInTheDocument();

    // Tangent card
    expect(screen.getAllByText('Leader Election').length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText('2 replies')).toBeInTheDocument();
  });

  it('rendering CourseTangentsSection groups tangents by course and allows jumping to Study Guide', async () => {
    vi.mocked(learningApi.listLearningTangents).mockResolvedValue([mockTangent, mockTangent2]);
    queryClient.setQueryData(learningTangentsKey(undefined), [mockTangent, mockTangent2]);
    queryClient.setQueryData(learningTangentsKey(null), [mockTangent, mockTangent2]);
    queryClient.setQueryData(learningTangentsKey(), [mockTangent, mockTangent2]);

    const onOpenDestination = vi.fn();
    const Wrapper = createTestWrapper(queryClient);

    render(
      <Wrapper>
        <CourseTangentsSection onOpenDestination={onOpenDestination} />
      </Wrapper>,
    );

    expect(screen.getByRole('region', { name: 'Course tangents' })).toBeInTheDocument();
    expect(screen.getByText('Distributed Systems')).toBeInTheDocument();
    expect(screen.getByText('Compiler Design')).toBeInTheDocument();

    // Jump to Study Guide on second course's tangent
    const studyGuideButtons = screen.getAllByRole('button', { name: 'Study Guide' });
    expect(studyGuideButtons.length).toBe(2);

    fireEvent.click(studyGuideButtons[1]);

    expect(onOpenDestination).toHaveBeenCalledWith({
      programId: 'program-2',
      moduleId: 'module-2',
      lessonId: 'lesson-2',
      tab: 'study',
      tangentId: 'tangent-conv-2',
    });
  });
});
