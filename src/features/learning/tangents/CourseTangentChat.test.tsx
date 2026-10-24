import type { ComponentProps, ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { fetchConversationDetail, fetchMessages } from '@/features/chat/api/conversationQueryData';
import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { learningTangentsKey } from '@/features/learning/api/learningQueryKeys';
import type { LearningTangentDto } from '@/lib/bindings';
import { toast } from '@/stores/toastStore';

import { CourseTangentChat } from './CourseTangentChat';

const mocks = vi.hoisted(() => ({
  navigate: vi.fn(),
  store: {} as Record<string, unknown>,
}));

vi.mock('react-router', async (importOriginal) => ({ ...(await importOriginal<typeof import('react-router')>()), useNavigate: () => mocks.navigate }));
vi.mock('@/shared/conversations/conversationsStore', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/shared/conversations/conversationsStore')>()),
  useConversationsStore: (selector?: (state: unknown) => unknown) => (selector ? selector(mocks.store) : mocks.store),
}));
vi.mock('@/features/chat/api/conversationQueryData', () => ({
  fetchMessages: vi.fn(),
  fetchConversationDetail: vi.fn(),
  fetchBookmarks: vi.fn().mockResolvedValue([]),
}));
vi.mock('@/features/chat/components/Message', () => ({
  Message: ({ message }: { message: { content: string } }) => <p data-testid="message">{message.content}</p>,
}));

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

function renderChat(props: Partial<ComponentProps<typeof CourseTangentChat>> = {}) {
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const invalidate = vi.spyOn(client, 'invalidateQueries');
  const handlers = { onBackToList: vi.fn(), onClose: vi.fn(), ...props };
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}><TooltipProvider>{children}</TooltipProvider></QueryClientProvider>
  );
  render(<CourseTangentChat tangent={tangent} currentModuleId="module-2" currentLessonId="lesson-5" {...handlers} />, { wrapper });
  return { ...handlers, invalidate };
}

const composer = () => screen.getByRole('textbox', { name: 'Ask in this tangent' });
const sendButton = () => screen.getByRole('button', { name: 'Send tangent message' });

beforeEach(() => {
  mocks.navigate.mockReset();
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
  it('shows where the tangent started and the replies after its context', async () => {
    renderChat();
    expect(screen.getByRole('status')).toHaveTextContent('Opening tangent…');
    expect(await screen.findByText('They checked each other.')).toBeInTheDocument();
    expect(screen.queryByText('Context about the Senate')).not.toBeInTheDocument();
    expect(screen.getByText('Tangent · Consuls')).toBeInTheDocument();
    expect(screen.getByText('Ancient Rome · The Republic')).toBeInTheDocument();
    expect(screen.getByText('The Republic › The Senate › Consuls')).toBeInTheDocument();
    expect(screen.getByText('Two consuls were elected each year.')).toBeInTheDocument();
    expect(mocks.store.selectConversation).toHaveBeenCalledWith('tangent-1');
    expect(screen.queryByRole('button', { name: 'Jump to lesson' })).not.toBeInTheDocument();
  });

  it('offers to jump to the lesson the tangent came from when another is open', async () => {
    const onJumpToLesson = vi.fn();
    renderChat({ currentLessonId: 'lesson-9', onJumpToLesson });
    fireEvent.click(screen.getByRole('button', { name: 'Jump to lesson' }));
    expect(onJumpToLesson).toHaveBeenCalledWith('module-2', 'lesson-5');
  });

  it('names the lesson when the tangent has no section', async () => {
    const props = { tangent: { ...tangent, sectionTitle: null } };
    client = new QueryClient();
    render(
      <QueryClientProvider client={client}><TooltipProvider>
        <CourseTangentChat {...props} onBackToList={vi.fn()} onClose={vi.fn()} />
      </TooltipProvider></QueryClientProvider>,
    );
    expect(screen.getByText('Tangent · The Senate')).toBeInTheDocument();
    expect(screen.getByText('The Republic › The Senate')).toBeInTheDocument();
  });

  it('starts an empty tangent from a suggested question', async () => {
    vi.mocked(fetchMessages).mockResolvedValue([seed] as never);
    const { invalidate } = renderChat();
    fireEvent.click(await screen.findByRole('button', { name: /Give me a concrete example/ }));
    await waitFor(() => expect(mocks.store.sendMessage).toHaveBeenCalledWith('Give me a concrete example', 'tangent-1'));
    await waitFor(() => expect(invalidate).toHaveBeenCalledWith({ queryKey: learningTangentsKey('course-1') }));
  });

  it('sends a typed question with Enter and clears the composer, but not with Shift+Enter', async () => {
    renderChat();
    await screen.findByText('They checked each other.');
    expect(sendButton()).toBeDisabled();
    fireEvent.change(composer(), { target: { value: '  Could a consul veto the other?  ' } });
    fireEvent.keyDown(composer(), { key: 'Enter', shiftKey: true });
    expect(mocks.store.sendMessage).not.toHaveBeenCalled();
    fireEvent.keyDown(composer(), { key: 'Enter' });
    await waitFor(() => expect(mocks.store.sendMessage).toHaveBeenCalledWith('Could a consul veto the other?', 'tangent-1'));
    expect(composer()).toHaveValue('');
  });

  it('puts the question back when the model is busy', async () => {
    (mocks.store.sendMessage as ReturnType<typeof vi.fn>).mockResolvedValue('busy');
    renderChat();
    await screen.findByText('They checked each other.');
    fireEvent.change(composer(), { target: { value: 'Who chose them?' } });
    fireEvent.click(sendButton());
    await waitFor(() => expect(composer()).toHaveValue('Who chose them?'));
  });

  it('puts back a failed question that never reached the transcript', async () => {
    (mocks.store.sendMessage as ReturnType<typeof vi.fn>).mockResolvedValue('failed');
    renderChat();
    await screen.findByText('They checked each other.');
    fireEvent.change(composer(), { target: { value: 'Who chose them?' } });
    fireEvent.click(sendButton());
    await waitFor(() => expect(composer()).toHaveValue('Who chose them?'));
  });

  it('keeps the composer clear when a failed question is already in the transcript', async () => {
    (mocks.store.sendMessage as ReturnType<typeof vi.fn>).mockImplementation(async (question: string) => {
      client.setQueryData(['conversations', 'messages', 'tangent-1'], [seed, message('q1', 'user', question)]);
      return 'failed';
    });
    renderChat();
    await screen.findByText('They checked each other.');
    fireEvent.change(composer(), { target: { value: 'Who chose them?' } });
    fireEvent.click(sendButton());
    await waitFor(() => expect(mocks.store.sendMessage).toHaveBeenCalled());
    expect(composer()).toHaveValue('');
  });

  it('reports a send that threw and restores the question', async () => {
    (mocks.store.sendMessage as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('no model loaded'));
    renderChat();
    await screen.findByText('They checked each other.');
    fireEvent.change(composer(), { target: { value: 'Who chose them?' } });
    fireEvent.click(sendButton());
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Couldn't send in this tangent", { message: 'no model loaded' }));
    expect(composer()).toHaveValue('Who chose them?');
  });

  it('can stop an answer being generated', async () => {
    mocks.store.inFlightGenerations = new Map([['tangent-1', {}]]);
    renderChat();
    expect(screen.getByText('Generating answer…')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Delete tangent' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'Stop tangent response' }));
    expect(mocks.store.cancelGeneration).toHaveBeenCalledWith('tangent-1');
  });

  it('opens the tangent in full Chat in its own space', async () => {
    renderChat();
    fireEvent.click(screen.getByRole('button', { name: 'Open in Chat' }));
    await waitFor(() => expect(mocks.navigate).toHaveBeenCalledWith('/chat?conversationId=tangent-1'));
    expect(mocks.store.setSelectedSpace).toHaveBeenCalledWith('space-history');
  });

  it('reports a tangent that could not be opened in Chat', async () => {
    renderChat();
    await waitFor(() => expect(mocks.store.selectConversation).toHaveBeenCalled());
    (mocks.store.selectConversation as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('conversation missing'));
    fireEvent.click(screen.getByRole('button', { name: /Open in full Chat/ }));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Couldn't open in chat", { message: 'conversation missing' }));
    expect(mocks.navigate).not.toHaveBeenCalled();
  });

  it('deletes the tangent only after confirmation', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValueOnce(false).mockReturnValueOnce(true);
    const { onBackToList, invalidate } = renderChat();
    await screen.findByText('They checked each other.');
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
    (mocks.store.deleteConversation as ReturnType<typeof vi.fn>).mockResolvedValue(false);
    conversationUiStore.setState({ error: 'Conversation is generating' } as never);
    const { onBackToList } = renderChat();
    fireEvent.click(screen.getByRole('button', { name: 'Delete tangent' }));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("Couldn't delete the tangent", { message: 'Conversation is generating' }));
    expect(onBackToList).not.toHaveBeenCalled();
  });

  it('offers a retry when the tangent cannot be opened', async () => {
    vi.mocked(fetchConversationDetail).mockResolvedValueOnce(null as never);
    renderChat();
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Couldn’t open this tangent.');
    expect(sendButton()).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(screen.queryByRole('alert')).not.toBeInTheDocument());
    expect(fetchMessages).toHaveBeenCalledTimes(2);
  });

  it('goes back to the list or closes the drawer', async () => {
    const { onBackToList, onClose } = renderChat();
    fireEvent.click(screen.getByRole('button', { name: 'All course tangents' }));
    fireEvent.click(screen.getByRole('button', { name: 'Close tangents drawer' }));
    expect(onBackToList).toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
    await act(async () => { await Promise.resolve(); });
  });
});
