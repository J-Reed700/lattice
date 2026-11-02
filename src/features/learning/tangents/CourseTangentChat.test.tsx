import type { ComponentProps, ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { fetchConversationDetail, fetchMessages } from '@/features/chat/api/conversationQueryData';
import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { learningTangentsKey } from '@/features/learning/api/learningQueryKeys';
import type { LearningTangentDto } from '@/lib/bindings';
import { ConversationSnapshotProvider } from '@/shared/conversations/conversationsStore';
import { toast } from '@/stores/toastStore';

import { CourseTangentChat } from './CourseTangentChat';

const mocks = vi.hoisted(() => ({
  navigate: vi.fn(),
  pick: vi.fn(),
  chatProps: null as null | Record<string, any>,
  store: {} as Record<string, any>,
}));

vi.mock('react-router', async (importOriginal) => ({ ...(await importOriginal<typeof import('react-router')>()), useNavigate: () => mocks.navigate }));
vi.mock('@/features/chat/api/conversationQueryData', () => ({
  fetchMessages: vi.fn(),
  fetchConversationDetail: vi.fn(),
  fetchBookmarks: vi.fn().mockResolvedValue([]),
}));
// Stands in for the shared chat: it shows whatever conversation the tangent scopes
// it to and, while that is empty, the note and starters its host gave it, where
// picking a starter fills the composer.
vi.mock('@/features/chat/components/ChatPanel', async () => {
  const { useConversationsStore } = await import('@/shared/conversations/conversationsStore');
  const none: Array<{ id: string; content: string }> = [];
  return {
    ChatPanel: (props: Record<string, any>) => {
      mocks.chatProps = props;
      const id = useConversationsStore((state) => state.activeConversationId);
      const thread = useConversationsStore((state) => state.conversations.find((conversation) => conversation.id === id)?.messages ?? none);
      return (
        <section aria-label="Shared chat" data-conversation={id}>
          {thread.map((item) => <p key={item.id}>{item.content}</p>)}
          {thread.length === 0 && props.startersNote && <p>{props.startersNote}</p>}
          {thread.length === 0 && props.starters?.map((starter: string) => (
            <button key={starter} type="button" onClick={() => mocks.pick(starter)}>{starter}</button>
          ))}
        </section>
      );
    },
  };
});

const tangent: LearningTangentDto = {
  conversationId: 'tangent-1', programId: 'course-1', programTitle: 'Ancient Rome', spaceId: 'space-history',
  moduleId: 'module-2', moduleTitle: 'The Republic', lessonId: 'lesson-5', lessonTitle: 'The Senate',
  sectionIndex: 1, sectionTitle: 'Consuls', selectedText: 'Two consuls were elected each year.', title: 'Why two?',
  createdAt: 1, updatedAt: '2026-10-20T00:00:00Z', messageCount: 3, contextMessageCount: 1,
};
const message = (id: string, role: 'user' | 'assistant', content: string) => ({
  id, conversationId: 'tangent-1', role, content, status: 'completed', createdAt: 1,
});
const seed = message('seed', 'user', 'Context about the Senate');

let client: QueryClient;

const Wrapper = ({ children }: { children: ReactNode }) => (
  <QueryClientProvider client={client}>
    <TooltipProvider>
      <ConversationSnapshotProvider value={mocks.store as never}>{children}</ConversationSnapshotProvider>
    </TooltipProvider>
  </QueryClientProvider>
);

function renderChat(props: Partial<ComponentProps<typeof CourseTangentChat>> = {}) {
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const invalidate = vi.spyOn(client, 'invalidateQueries');
  const handlers = { onBackToList: vi.fn(), onClose: vi.fn(), ...props };
  const chat = <CourseTangentChat tangent={tangent} currentModuleId="module-2" currentLessonId="lesson-5" {...handlers} />;
  const view = render(chat, { wrapper: Wrapper });
  return { ...view, ...handlers, invalidate, rerenderChat: () => view.rerender(chat) };
}

const sharedChat = () => screen.findByRole('region', { name: 'Shared chat' });

beforeEach(() => {
  mocks.navigate.mockReset();
  mocks.pick.mockReset();
  mocks.chatProps = null;
  mocks.store = {
    inFlightGenerations: new Map(),
    optimisticMessages: new Map(),
    selectConversation: vi.fn().mockResolvedValue(undefined),
    sendMessage: vi.fn().mockResolvedValue('answered'),
    cancelGeneration: vi.fn().mockResolvedValue(undefined),
    deleteConversation: vi.fn().mockResolvedValue(true),
    setSelectedSpace: vi.fn(),
    regenerateResponse: vi.fn(),
  };
  vi.mocked(fetchMessages).mockReset().mockResolvedValue([seed, message('a1', 'assistant', 'They checked each other.')] as never);
  vi.mocked(fetchConversationDetail).mockReset().mockResolvedValue({ id: 'tangent-1', title: 'Why two?', spaceId: 'space-history', messages: [] } as never);
  conversationUiStore.setState({ optimisticMessages: new Map(), error: null } as never);
  vi.spyOn(toast, 'success');
  vi.spyOn(toast, 'error');
});
afterEach(() => vi.restoreAllMocks());

describe('CourseTangentChat', () => {
  it('shows where the tangent started, then the shared chat on the replies after its context', async () => {
    renderChat();
    expect(screen.getByRole('status')).toHaveTextContent('Opening tangent…');
    const chat = await sharedChat();
    expect(chat).toHaveAttribute('data-conversation', 'tangent-1');
    expect(within(chat).getByText('They checked each other.')).toBeInTheDocument();
    expect(screen.queryByText('Context about the Senate')).not.toBeInTheDocument();
    expect(mocks.chatProps).toMatchObject({ embedded: true, autoFocus: true });
    expect(screen.getByText('Tangent · Consuls')).toBeInTheDocument();
    expect(screen.getByText('Ancient Rome · The Republic')).toBeInTheDocument();
    expect(screen.getByText('The Republic › The Senate › Consuls')).toBeInTheDocument();
    expect(screen.getByText('Two consuls were elected each year.')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Jump to lesson' })).not.toBeInTheDocument();
  });

  it('is not opened in Chat, which would change the conversation Chat has open', async () => {
    renderChat();
    await sharedChat();
    expect(mocks.store.selectConversation).not.toHaveBeenCalled();
    expect(mocks.store.setSelectedSpace).not.toHaveBeenCalled();
  });

  it('always keeps the first message, which sets the scene, out of the thread', async () => {
    renderChat({ tangent: { ...tangent, contextMessageCount: 0 } });
    const chat = await sharedChat();
    expect(within(chat).getByText('They checked each other.')).toBeInTheDocument();
    expect(screen.queryByText('Context about the Senate')).not.toBeInTheDocument();
  });

  it('offers to jump to the lesson the tangent came from when another is open', async () => {
    const onJumpToLesson = vi.fn();
    renderChat({ currentLessonId: 'lesson-9', onJumpToLesson });
    fireEvent.click(screen.getByRole('button', { name: 'Jump to lesson' }));
    expect(onJumpToLesson).toHaveBeenCalledWith('module-2', 'lesson-5');
    await sharedChat();
  });

  it('names the lesson when the tangent has no section', async () => {
    renderChat({ tangent: { ...tangent, sectionTitle: null } });
    expect(screen.getByText('Tangent · The Senate')).toBeInTheDocument();
    expect(screen.getByText('The Republic › The Senate')).toBeInTheDocument();
    await sharedChat();
  });

  it('gives an empty tangent a line about the passage and three starters, as data', async () => {
    vi.mocked(fetchMessages).mockResolvedValue([seed] as never);
    renderChat();
    expect(await screen.findByText('Ask a question or explore a side-topic about this study guide passage.')).toBeInTheDocument();
    expect(mocks.chatProps).toMatchObject({
      startersNote: 'Ask a question or explore a side-topic about this study guide passage.',
      starters: ['Explain this intuitively', 'Give me a concrete example', 'How does this connect to the module goal?'],
    });
    expect(screen.getAllByRole('button', { name: /intuitively|concrete example|module goal/ })).toHaveLength(3);
  });

  it('fills the composer with a starter rather than sending it', async () => {
    vi.mocked(fetchMessages).mockResolvedValue([seed] as never);
    renderChat();
    fireEvent.click(await screen.findByRole('button', { name: 'Give me a concrete example' }));
    expect(mocks.pick).toHaveBeenCalledWith('Give me a concrete example');
    expect(mocks.store.sendMessage).not.toHaveBeenCalled();
  });

  it('refreshes the course’s tangents when a turn ends', async () => {
    mocks.store = { ...mocks.store, inFlightGenerations: new Map([['tangent-1', 'r1']]) };
    const { invalidate, rerenderChat } = renderChat();
    await sharedChat();
    invalidate.mockClear();
    mocks.store = { ...mocks.store, inFlightGenerations: new Map() };
    rerenderChat();
    await waitFor(() => expect(invalidate).toHaveBeenCalledWith({ queryKey: learningTangentsKey('course-1') }));
  });

  it('opens the tangent in full Chat in its own space', async () => {
    renderChat();
    fireEvent.click(screen.getByRole('button', { name: 'Open in Chat' }));
    await waitFor(() => expect(mocks.navigate).toHaveBeenCalledWith('/chat?conversationId=tangent-1'));
    expect(mocks.store.setSelectedSpace).toHaveBeenCalledWith('space-history');
    expect(mocks.store.selectConversation).toHaveBeenCalledWith('tangent-1');
    await sharedChat();
  });

  it('reports a tangent that could not be opened in Chat', async () => {
    mocks.store.selectConversation.mockRejectedValue(new Error('conversation missing'));
    renderChat();
    fireEvent.click(screen.getByRole('button', { name: /Open in full Chat/ }));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Couldn't open in chat", { message: 'conversation missing' }));
    expect(mocks.navigate).not.toHaveBeenCalled();
    await sharedChat();
  });

  it('deletes the tangent only after confirmation', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValueOnce(false).mockReturnValueOnce(true);
    const { onBackToList, invalidate } = renderChat();
    await sharedChat();
    fireEvent.click(screen.getByRole('button', { name: 'Delete tangent' }));
    expect(confirm).toHaveBeenCalledWith('Delete this tangent and its messages?');
    expect(mocks.store.deleteConversation).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Delete tangent' }));
    await waitFor(() => expect(onBackToList).toHaveBeenCalled());
    expect(mocks.store.deleteConversation).toHaveBeenCalledWith('tangent-1');
    expect(toast.success).toHaveBeenCalledWith('Tangent deleted');
    expect(invalidate).toHaveBeenCalledWith({ queryKey: learningTangentsKey(null) });
  });

  it('says why a tangent could not be deleted', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    mocks.store.deleteConversation.mockResolvedValue(false);
    conversationUiStore.setState({ error: 'Conversation is generating' } as never);
    const { onBackToList } = renderChat();
    fireEvent.click(screen.getByRole('button', { name: 'Delete tangent' }));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Couldn't delete the tangent", { message: 'Conversation is generating' }));
    expect(onBackToList).not.toHaveBeenCalled();
    await sharedChat();
  });

  it('will not delete a tangent that is writing a reply', async () => {
    mocks.store = { ...mocks.store, inFlightGenerations: new Map([['tangent-1', 'r1']]) };
    renderChat();
    expect(screen.getByRole('button', { name: 'Delete tangent' })).toBeDisabled();
    await sharedChat();
  });

  it('offers a retry when the tangent cannot be opened, with no chat to type into', async () => {
    vi.mocked(fetchConversationDetail).mockResolvedValueOnce(null as never);
    renderChat();
    expect(await screen.findByRole('alert')).toHaveTextContent('Couldn’t open this tangent.');
    expect(screen.queryByRole('region', { name: 'Shared chat' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await sharedChat();
    expect(fetchMessages).toHaveBeenCalledTimes(2);
  });

  it('goes back to the list or closes the drawer', async () => {
    const { onBackToList, onClose } = renderChat();
    fireEvent.click(screen.getByRole('button', { name: 'All course tangents' }));
    fireEvent.click(screen.getByRole('button', { name: 'Close tangents drawer' }));
    expect(onBackToList).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
    await sharedChat();
  });
});
