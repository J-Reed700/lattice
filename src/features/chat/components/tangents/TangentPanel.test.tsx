import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { ConversationSnapshotProvider, useConversationsStore, useThreadStartsAfter } from '@/shared/conversations/conversationsStore';

import { TangentPanel } from './TangentPanel';

const h = vi.hoisted(() => ({
  base: {} as Record<string, any>,
  chatProps: null as null | Record<string, any>,
  fetchMessages: vi.fn(),
  fetchConversationDetail: vi.fn(),
  fetchBookmarks: vi.fn(),
  promote: vi.fn(),
  toast: { success: vi.fn(), error: vi.fn() },
}));

vi.mock('@/features/chat/api/conversationQueryData', () => ({
  fetchMessages: h.fetchMessages,
  fetchConversationDetail: h.fetchConversationDetail,
  fetchBookmarks: h.fetchBookmarks,
}));
vi.mock('@/features/chat/hooks/useConversationTangents', () => ({ usePromoteTangentMutation: () => ({ mutateAsync: h.promote }) }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

const NO_MESSAGES: Array<{ id: string; content: string }> = [];

// Stands in for the shared chat: it shows whatever conversation the panel scopes it to.
function Chat(props: Record<string, any>) {
  h.chatProps = props;
  const id = useConversationsStore((state) => state.activeConversationId);
  const thread = useConversationsStore((state) => state.conversations.find((conversation) => conversation.id === id)?.messages ?? NO_MESSAGES);
  return (
    <section aria-label="Shared chat" data-conversation={id}>
      {thread.map((item) => <p key={item.id}>{item.content}</p>)}
      {thread.length === 0 && props.renderEmptyState?.(vi.fn())}
    </section>
  );
}

// A chat that says where it was told its thread begins.
function FollowingChat() {
  return <p data-follows={useThreadStartsAfter() ?? 'nothing'}>Chat inside</p>;
}

const tangent = {
  conversationId: 't1',
  parentConversationId: 'p1',
  sourceConversationId: 'p1',
  sourceMessageId: 'm2',
  selectedText: 'The Hanseatic League controlled Baltic trade.',
  title: 'Hansa',
  createdAt: '',
  updatedAt: '',
  contextMessageCount: 2,
} as never;

const message = (id: string, role: 'user' | 'assistant', content: string) => ({ id, conversationId: 't1', role, content, createdAt: '2026-01-01T00:00:00Z' });
const context = [message('c1', 'user', 'Tell me about medieval trade'), message('c2', 'assistant', 'Trade routes grew…')];

let queryClient: QueryClient;
const panel = (onPromoted: () => void) => <TangentPanel tangent={tangent} chat={Chat} onPromoted={onPromoted} />;
const wrapper = ({ children }: { children: ReactNode }) => (
  <QueryClientProvider client={queryClient}>
    <TooltipProvider>
      <ConversationSnapshotProvider value={h.base as never}>{children}</ConversationSnapshotProvider>
    </TooltipProvider>
  </QueryClientProvider>
);
const renderPanel = () => {
  const onPromoted = vi.fn();
  const view = render(panel(onPromoted), { wrapper });
  return { ...view, onPromoted };
};

const ready = () => screen.findByRole('region', { name: 'Shared chat' });

describe('TangentPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    h.chatProps = null;
    h.base = {
      inFlightGenerations: new Map(),
      optimisticMessages: new Map(),
      cancelGeneration: vi.fn(),
      regenerateResponse: vi.fn().mockResolvedValue('answered'),
      deleteConversation: vi.fn().mockResolvedValue(true),
      selectConversation: vi.fn(),
    };
    h.fetchMessages.mockResolvedValue(context);
    h.fetchConversationDetail.mockResolvedValue({ id: 't1', title: 'Hansa' });
    h.fetchBookmarks.mockResolvedValue([]);
    h.promote.mockResolvedValue(undefined);
    conversationUiStore.setState({ optimisticMessages: new Map(), error: null } as never);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('the chat', () => {
    it('shows the passage, then the shared chat on the tangent’s own thread', async () => {
      h.fetchMessages.mockResolvedValue([...context, message('m3', 'user', 'Which cities joined?'), message('m4', 'assistant', 'Lübeck led it.')]);
      renderPanel();
      expect(screen.getByText('The Hanseatic League controlled Baltic trade.')).toBeInTheDocument();
      expect(screen.getByRole('status')).toHaveTextContent('Opening tangent…');
      const chat = await ready();
      expect(chat).toHaveAttribute('data-conversation', 't1');
      expect(screen.getByText('Lübeck led it.')).toBeInTheDocument();
      expect(screen.getByText('Which cities joined?')).toBeInTheDocument();
      expect(screen.queryByText('Tell me about medieval trade')).not.toBeInTheDocument();
    });

    it('asks to be embedded and to open on its composer', async () => {
      renderPanel();
      await ready();
      expect(h.chatProps).toMatchObject({ embedded: true, autoFocus: true });
    });

    it('has its thread follow the last message the tangent inherited, where a branch before the first question forks', async () => {
      render(<TangentPanel tangent={tangent} chat={FollowingChat} onPromoted={vi.fn()} />, { wrapper });
      expect(await screen.findByText('Chat inside')).toHaveAttribute('data-follows', 'c2');
    });

    it('invites a question about the passage in an empty tangent', async () => {
      renderPanel();
      await ready();
      expect(screen.getByText('Ask about this passage. You can return to the main conversation whenever you’re ready.')).toBeInTheDocument();
    });

    it('offers a retry when the tangent cannot be opened, with no chat to type into', async () => {
      h.fetchMessages.mockRejectedValueOnce(new Error('db locked'));
      renderPanel();
      expect(await screen.findByRole('alert')).toHaveTextContent('Couldn’t open this tangent.');
      expect(screen.queryByRole('region', { name: 'Shared chat' })).not.toBeInTheDocument();
      fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
      await ready();
      expect(h.fetchMessages).toHaveBeenCalledTimes(2);
    });

    it('refreshes its parent’s tangents when a turn ends', async () => {
      h.base.inFlightGenerations = new Map([['t1', 'r1']]);
      const view = renderPanel();
      await ready();
      vi.spyOn(queryClient, 'invalidateQueries');
      expect(queryClient.invalidateQueries).not.toHaveBeenCalled();
      h.base = { ...h.base, inFlightGenerations: new Map() };
      view.rerender(panel(vi.fn()));
      await waitFor(() => expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: conversationKeys.tangents('p1') }));
    });
  });

  describe('finishing', () => {
    it('makes the tangent a conversation and drops it from the list', async () => {
      queryClient.setQueryData(conversationKeys.tangents('p1'), [{ conversationId: 't1' }, { conversationId: 't2' }]);
      const { onPromoted } = renderPanel();
      await ready();
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: /Make conversation/ })); });
      expect(h.promote).toHaveBeenCalledWith(tangent);
      expect(onPromoted).toHaveBeenCalled();
      expect(queryClient.getQueryData(conversationKeys.tangents('p1'))).toEqual([{ conversationId: 't2' }]);
      const [, options] = h.toast.success.mock.calls[0];
      options.action.onClick();
      expect(h.base.selectConversation).toHaveBeenCalledWith('t1');
    });

    it('reports a promotion that failed', async () => {
      h.promote.mockRejectedValueOnce(new Error('title taken'));
      const { onPromoted } = renderPanel();
      await ready();
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: /Make conversation/ })); });
      expect(h.toast.error).toHaveBeenCalledWith("Couldn't make this a conversation", { message: 'title taken' });
      expect(onPromoted).not.toHaveBeenCalled();
    });

    it('deletes only after confirmation', async () => {
      const confirm = vi.spyOn(window, 'confirm').mockReturnValueOnce(false).mockReturnValueOnce(true);
      const { onPromoted } = renderPanel();
      await ready();
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Delete tangent' })); });
      expect(h.base.deleteConversation).not.toHaveBeenCalled();
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Delete tangent' })); });
      expect(confirm).toHaveBeenCalledWith('Delete this tangent and its messages?');
      expect(h.base.deleteConversation).toHaveBeenCalledWith('t1');
      expect(onPromoted).toHaveBeenCalled();
    });

    it('reports a delete the store refused, with its reason', async () => {
      vi.spyOn(window, 'confirm').mockReturnValue(true);
      h.base.deleteConversation.mockImplementationOnce(async () => {
        conversationUiStore.setState({ error: 'conversation is locked' } as never);
        return false;
      });
      renderPanel();
      await ready();
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Delete tangent' })); });
      expect(h.toast.error).toHaveBeenCalledWith("Couldn't delete the tangent", { message: 'conversation is locked' });
      conversationUiStore.setState({ error: null } as never);
      h.base.deleteConversation.mockResolvedValueOnce(false);
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Delete tangent' })); });
      expect(h.toast.error).toHaveBeenLastCalledWith("Couldn't delete the tangent", { message: 'Please try again.' });
    });

    it('does neither while a reply is being written', async () => {
      h.base.inFlightGenerations = new Map([['t1', 'r1']]);
      renderPanel();
      await ready();
      expect(screen.getByRole('button', { name: /Make conversation/ })).toBeDisabled();
      expect(screen.getByRole('button', { name: 'Delete tangent' })).toBeDisabled();
    });
  });
});
