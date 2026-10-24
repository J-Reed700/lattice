import { useContext, type ComponentProps } from 'react';

import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import type { LearningProgramDto, LearningTangentDto } from '@/lib/bindings';
import { toast } from '@/stores/toastStore';

import { ProgramTangents, ProgramTangentsTrigger } from './ProgramTangents';
import { StudyTangentsContext } from './StudyTangentSelection';

const mocks = vi.hoisted(() => ({
  query: { data: [] as unknown[], isPending: false, isError: false, isSuccess: true, refetch: vi.fn() },
  create: { mutateAsync: vi.fn(), isPending: false },
  navigate: vi.fn(),
  store: null as null | { setSelectedSpace: ReturnType<typeof vi.fn>; selectConversation: ReturnType<typeof vi.fn> },
}));
vi.mock('./useLearningTangents', () => ({
  useLearningTangents: () => mocks.query,
  useCreateLearningTangent: () => mocks.create,
}));
vi.mock('react-router', async (importOriginal) => ({ ...(await importOriginal<typeof import('react-router')>()), useNavigate: () => mocks.navigate }));
vi.mock('@/shared/conversations/conversationsStore', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/shared/conversations/conversationsStore')>()),
  useOptionalConversationsStore: () => mocks.store,
}));
vi.mock('./CourseTangentChat', () => ({
  CourseTangentChat: ({ tangent, onBackToList, onClose, onJumpToLesson }: { tangent: LearningTangentDto; onBackToList: () => void; onClose: () => void; onJumpToLesson: (m: string, l: string) => void }) => (
    <div data-testid="tangent-chat">
      <p>Chat: {tangent.title}</p>
      <button type="button" onClick={onBackToList}>Back to list</button>
      <button type="button" onClick={onClose}>Close chat</button>
      <button type="button" onClick={() => onJumpToLesson(tangent.moduleId, tangent.lessonId)}>Jump</button>
    </div>
  ),
}));

const program = {
  summary: { id: 'course-1', title: 'Ancient Rome' },
  modules: [
    { id: 'm1', title: 'Kings', lessons: [{ id: 'l1', title: 'Romulus' }, { id: 'l2', title: 'Numa' }] },
    { id: 'm2', title: 'Republic', lessons: [{ id: 'l3', title: 'The Senate' }] },
  ],
} as unknown as LearningProgramDto;

const tangent = (id: string, overrides: Partial<LearningTangentDto> = {}): LearningTangentDto => ({
  conversationId: id, programId: 'course-1', programTitle: 'Ancient Rome', spaceId: 'space-1', moduleId: 'm1', moduleTitle: 'Kings',
  lessonId: 'l1', lessonTitle: 'Romulus', sectionIndex: null, sectionTitle: null, selectedText: 'The twins were raised by a wolf.',
  title: `Tangent ${id}`, createdAt: 1, updatedAt: '2026-10-20T00:00:00Z', messageCount: 3, contextMessageCount: 1, ...overrides,
});

function Starter() {
  const context = useContext(StudyTangentsContext)!;
  return (
    <>
      <button type="button" onClick={() => void context.startTangent({ moduleId: 'm1', lessonId: 'l1', selectedText: 'wolf' })}>Start from passage</button>
      <button type="button" onClick={() => context.openTangentsDrawer('l1')}>Open lesson tangents</button>
      <ProgramTangentsTrigger />
    </>
  );
}

function renderTangents(props: Partial<ComponentProps<typeof ProgramTangents>> = {}) {
  const onSelectLesson = vi.fn();
  const view = render(
    <TooltipProvider>
      <ProgramTangents program={program} currentModuleId="m1" currentLessonId="l1" onSelectLesson={onSelectLesson} {...props}>
        <Starter />
      </ProgramTangents>
    </TooltipProvider>,
  );
  return { ...view, onSelectLesson };
}

const drawer = () => screen.queryByRole('complementary', { name: 'Course tangents' });
const trigger = () => screen.getByRole('button', { name: 'Course tangents' });

beforeEach(() => {
  mocks.query = { data: [], isPending: false, isError: false, isSuccess: true, refetch: vi.fn() };
  mocks.create = { mutateAsync: vi.fn().mockResolvedValue(tangent('new')), isPending: false };
  mocks.navigate.mockReset();
  mocks.store = { setSelectedSpace: vi.fn(), selectConversation: vi.fn().mockResolvedValue(undefined) };
  vi.spyOn(toast, 'error');
});
afterEach(() => vi.restoreAllMocks());

describe('ProgramTangents drawer', () => {
  it('opens and closes from the trigger and with Escape', () => {
    mocks.query.data = [tangent('a')];
    renderTangents();
    expect(drawer()).not.toBeInTheDocument();
    expect(trigger()).toHaveAttribute('aria-expanded', 'false');
    expect(trigger()).toHaveTextContent('1');
    fireEvent.click(trigger());
    expect(drawer()).toBeInTheDocument();
    expect(trigger()).toHaveAttribute('aria-expanded', 'true');
    fireEvent.keyDown(within(drawer()!).getByRole('heading', { name: 'Course Tangents' }), { key: 'Escape' });
    expect(drawer()).not.toBeInTheDocument();
    fireEvent.click(trigger());
    fireEvent.click(trigger());
    expect(drawer()).not.toBeInTheDocument();
  });

  it('opens on the tangent a link asks for', () => {
    mocks.query.data = [tangent('a'), tangent('b')];
    renderTangents({ initialTangentId: 'b' });
    expect(screen.getByText('Chat: Tangent b')).toBeInTheDocument();
  });

  it('starts a tangent from a passage and opens it', async () => {
    renderTangents();
    fireEvent.click(screen.getByRole('button', { name: 'Start from passage' }));
    await waitFor(() => expect(drawer()).toBeInTheDocument());
    expect(mocks.create.mutateAsync).toHaveBeenCalledWith({ programId: 'course-1', moduleId: 'm1', lessonId: 'l1', sectionIndex: null, sectionTitle: null, selectedText: 'wolf' });
  });

  it('reports a tangent that could not be started', async () => {
    mocks.create.mutateAsync.mockRejectedValue('model unavailable');
    renderTangents();
    fireEvent.click(screen.getByRole('button', { name: 'Start from passage' }));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Couldn't start the tangent", { message: 'model unavailable' }));
    expect(drawer()).not.toBeInTheDocument();
  });

  it('lists this lesson’s tangents, or every one, grouped by module and lesson', () => {
    mocks.query.data = [
      tangent('a', { sectionIndex: 1 }),
      tangent('b', { lessonId: 'l3', lessonTitle: 'The Senate', moduleId: 'm2', moduleTitle: 'Republic', sectionTitle: 'Consuls', messageCount: 2 }),
      tangent('c', { lessonId: 'lx', lessonTitle: 'Removed lesson', moduleId: 'mx', moduleTitle: 'Old module', messageCount: 1 }),
    ];
    renderTangents();
    fireEvent.click(screen.getByRole('button', { name: 'Open lesson tangents' }));
    const panel = drawer()!;
    expect(within(panel).getByRole('button', { name: 'This Lesson (1)' })).toBeInTheDocument();
    expect(within(panel).getByText('Section 2')).toBeInTheDocument();
    expect(within(panel).queryByText('Tangent b')).not.toBeInTheDocument();

    fireEvent.click(within(panel).getByRole('button', { name: 'All in Course (3)' }));
    expect(within(panel).getAllByRole('heading', { level: 3 }).map((h) => h.textContent)).toEqual(['Kings', 'Republic', 'Old module']);
    expect(within(panel).getByText('Removed lesson')).toBeInTheDocument();
    expect(within(panel).getByText('Consuls')).toBeInTheDocument();
    expect(within(panel).getByText('1 reply')).toBeInTheDocument();
    expect(within(panel).getByText('0 replies')).toBeInTheDocument();

    fireEvent.click(within(panel).getByRole('button', { name: 'This Lesson (1)' }));
    expect(within(panel).queryByText('Tangent b')).not.toBeInTheDocument();
  });

  it('searches tangents once there are more than three', () => {
    mocks.query.data = [
      tangent('a', { title: 'Wolf myth' }),
      tangent('b', { title: 'Founding date', selectedText: 'In 753 BC' }),
      tangent('c', { title: 'Remus', sectionTitle: 'The quarrel' }),
      tangent('d', { title: 'Other', lessonTitle: 'Romulus' }),
    ];
    renderTangents({ currentLessonId: null });
    fireEvent.click(trigger());
    const search = within(drawer()!).getByRole('textbox', { name: 'Search tangents' });
    fireEvent.change(search, { target: { value: '753' } });
    expect(within(drawer()!).getByText('Founding date')).toBeInTheDocument();
    expect(within(drawer()!).queryByText('Wolf myth')).not.toBeInTheDocument();
    fireEvent.change(search, { target: { value: 'quarrel' } });
    expect(within(drawer()!).getByText('Remus')).toBeInTheDocument();
    fireEvent.change(search, { target: { value: 'kings' } });
    expect(within(drawer()!).getAllByRole('button', { name: /Open tangent/ })).toHaveLength(4);
    fireEvent.change(search, { target: { value: 'carthage' } });
    expect(within(drawer()!).getByText(/to branch a side conversation/)).toBeInTheDocument();
  });

  it('opens a tangent card by click or keyboard, and goes back to the list', () => {
    mocks.query.data = [tangent('a'), tangent('b')];
    const { onSelectLesson } = renderTangents();
    fireEvent.click(trigger());
    const [first, second] = within(drawer()!).getAllByRole('button', { name: /Open tangent/ });
    fireEvent.keyDown(second, { key: 'Enter' });
    expect(screen.getByText('Chat: Tangent b')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Jump' }));
    expect(onSelectLesson).toHaveBeenCalledWith('m1', 'l1');
    fireEvent.click(screen.getByRole('button', { name: 'Back to list' }));
    expect(first).not.toBeInTheDocument();
    fireEvent.keyDown(within(drawer()!).getAllByRole('button', { name: /Open tangent/ })[0], { key: ' ' });
    expect(screen.getByText('Chat: Tangent a')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Close chat' }));
    expect(drawer()).not.toBeInTheDocument();
  });

  it('opens a tangent in full Chat from its card without opening it here', () => {
    mocks.query.data = [tangent('a')];
    renderTangents();
    fireEvent.click(trigger());
    fireEvent.click(within(drawer()!).getByTitle('Open in Chat'));
    expect(mocks.store!.setSelectedSpace).toHaveBeenCalledWith('space-1');
    expect(mocks.store!.selectConversation).toHaveBeenCalledWith('a');
    expect(mocks.navigate).toHaveBeenCalledWith('/chat?conversationId=a');
    expect(screen.queryByTestId('tangent-chat')).not.toBeInTheDocument();
  });

  it('navigates to Chat even with no conversation store mounted', () => {
    mocks.store = null;
    mocks.query.data = [tangent('a')];
    renderTangents();
    fireEvent.click(trigger());
    fireEvent.click(within(drawer()!).getByTitle('Open in Chat'));
    expect(mocks.navigate).toHaveBeenCalledWith('/chat?conversationId=a');
  });

  it('shows loading and a retry for a list that failed', () => {
    mocks.query = { data: undefined as never, isPending: true, isError: false, isSuccess: false, refetch: vi.fn() };
    const { rerender } = renderTangents();
    fireEvent.click(trigger());
    expect(screen.getByRole('status')).toHaveTextContent('Loading tangents…');
    mocks.query = { ...mocks.query, isPending: false, isError: true };
    rerender(
      <TooltipProvider>
        <ProgramTangents program={program} currentModuleId="m1" currentLessonId="l1"><Starter /></ProgramTangents>
      </TooltipProvider>,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(mocks.query.refetch).toHaveBeenCalled();
  });

  it('closes from the drawer header', async () => {
    renderTangents();
    fireEvent.click(trigger());
    fireEvent.click(screen.getByRole('button', { name: 'Close tangents' }));
    expect(drawer()).not.toBeInTheDocument();
    await act(async () => { await Promise.resolve(); });
  });
});
