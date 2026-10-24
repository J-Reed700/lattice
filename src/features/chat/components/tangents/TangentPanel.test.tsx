import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';
import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { conversationDraftKey, useDraftStore } from '@/shared/drafts/drafts';
import { deferred } from '@/tests/deferred';

import { TangentPanel } from './TangentPanel';

const h = vi.hoisted(() => ({
  base: {} as Record<string, any>,
  scope: null as null | Record<string, any>,
  fetchMessages: vi.fn(),
  fetchConversationDetail: vi.fn(),
  fetchBookmarks: vi.fn(),
  promote: vi.fn(),
  toast: { success: vi.fn(), error: vi.fn() },
}));

vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: () => h.base,
  ConversationSnapshotProvider: ({ value, children }: { value: Record<string, any>; children: ReactNode }) => {
    h.scope = value;
    return <>{children}</>;
  },
}));
vi.mock('@/features/chat/api/conversationQueryData', () => ({
  fetchMessages: h.fetchMessages,
  fetchConversationDetail: h.fetchConversationDetail,
  fetchBookmarks: h.fetchBookmarks,
}));
vi.mock('@/features/chat/hooks/useConversationTangents', () => ({ usePromoteTangentMutation: () => ({ mutateAsync: h.promote }) }));
vi.mock('@/features/chat/components/Message', () => ({
  Message: ({ message, isLastTurn, previousMessageId }: { message: { content: string }; isLastTurn: boolean; previousMessageId?: string }) => (
    <p data-last={isLastTurn} data-previous={previousMessageId ?? ''}>{message.content}</p>
  ),
}));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

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
const renderPanel = (props: Partial<{ unavailable: boolean; onPromoted: () => void }> = {}) => {
  const onPromoted = props.onPromoted ?? vi.fn();
  const view = render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <TangentPanel tangent={tangent} toolPreferences={{ web: true } as never} unavailable={props.unavailable ?? false} onPromoted={onPromoted} />
      </TooltipProvider>
    </QueryClientProvider>,
  );
  return { ...view, onPromoted };
};

const textarea = () => screen.getByLabelText('Ask in this tangent');
const draft = () => useDraftStore.getState().drafts[conversationDraftKey('t1')] ?? '';

const ready = async () => { await screen.findByText('Ask about this passage. You can return to the main conversation whenever you’re ready.'); };

describe('TangentPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    h.scope = null;
    h.base = {
      inFlightGenerations: new Map(),
      optimisticMessages: new Map(),
      sendMessage: vi.fn().mockResolvedValue('answered'),
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

  it('shows the passage and opens on the composer', async () => {
    renderPanel();
    expect(screen.getByText('The Hanseatic League controlled Baltic trade.')).toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('Opening tangent…');
    expect(textarea()).toHaveFocus();
    await ready();
    expect(screen.queryByText('Tell me about medieval trade')).not.toBeInTheDocument();
    expect(screen.getByText('Saved with this conversation')).toBeInTheDocument();
  });

  it('shows the tangent’s own messages after the inherited context, with pending ones', async () => {
    h.fetchMessages.mockResolvedValue([...context, message('m3', 'user', 'Which cities joined?'), message('m4', 'assistant', 'Lübeck led it.')]);
    h.base.optimisticMessages = new Map([
      ['tmp', { tempId: 'tmp', conversationId: 't1', role: 'user', content: 'And Bergen?' }],
      ['other', { tempId: 'other', conversationId: 'elsewhere', role: 'user', content: 'Not here' }],
    ]);
    renderPanel();
    expect(await screen.findByText('Lübeck led it.')).toHaveAttribute('data-previous', 'm3');
    expect(screen.getByText('Which cities joined?')).toHaveAttribute('data-previous', 'c2');
    expect(screen.getByText('And Bergen?')).toHaveAttribute('data-last', 'true');
    expect(screen.queryByText('Not here')).not.toBeInTheDocument();
  });

  it('offers a retry when the tangent cannot be opened', async () => {
    h.fetchMessages.mockRejectedValueOnce(new Error('db locked'));
    renderPanel();
    expect(await screen.findByRole('alert')).toHaveTextContent('Couldn’t open this tangent.');
    fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
    await ready();
    expect(h.fetchMessages).toHaveBeenCalledTimes(2);
  });

  it('treats a missing conversation as unopenable', async () => {
    h.fetchConversationDetail.mockResolvedValueOnce(null);
    renderPanel();
    expect(await screen.findByRole('alert')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Send tangent message' })).toBeDisabled();
  });

  describe('sending', () => {
    it('sends on Enter with the tangent’s tools and clears the draft', async () => {
      vi.spyOn(queryClient, 'invalidateQueries');
      renderPanel();
      await ready();
      fireEvent.change(textarea(), { target: { value: '  Why did it decline?  ' } });
      fireEvent.keyDown(textarea(), { key: 'Enter', shiftKey: true });
      expect(h.base.sendMessage).not.toHaveBeenCalled();
      await act(async () => { fireEvent.keyDown(textarea(), { key: 'Enter' }); });
      expect(h.base.sendMessage).toHaveBeenCalledWith('Why did it decline?', 't1', { web: true });
      expect(draft()).toBe('');
      expect(queryClient.invalidateQueries).toHaveBeenCalledWith({ queryKey: conversationKeys.tangents('p1') });
    });

    it('keeps the question when the model is busy', async () => {
      h.base.sendMessage.mockResolvedValueOnce('busy');
      renderPanel();
      await ready();
      fireEvent.change(textarea(), { target: { value: 'Why did it decline?' } });
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Send tangent message' })); });
      expect(draft()).toBe('Why did it decline?');
    });

    it('keeps the question when a failed send left no trace of it', async () => {
      h.base.sendMessage.mockResolvedValueOnce('failed');
      renderPanel();
      await ready();
      fireEvent.change(textarea(), { target: { value: 'Why did it decline?' } });
      await act(async () => { fireEvent.submit(textarea().closest('form')!); });
      expect(draft()).toBe('Why did it decline?');
    });

    it('lets the failed question stand when it is already in the thread', async () => {
      h.base.sendMessage.mockImplementationOnce(async () => {
        queryClient.setQueryData(conversationKeys.messages('t1'), [...context, message('m5', 'user', 'Why did it decline?')]);
        return 'failed';
      });
      renderPanel();
      await ready();
      fireEvent.change(textarea(), { target: { value: 'Why did it decline?' } });
      await act(async () => { fireEvent.submit(textarea().closest('form')!); });
      expect(draft()).toBe('');
    });

    it('lets the failed question stand when it is pending', async () => {
      h.base.sendMessage.mockImplementationOnce(async () => {
        conversationUiStore.setState({ optimisticMessages: new Map([['tmp', { tempId: 'tmp', conversationId: 't1', role: 'user', content: 'Why did it decline?' }]]) } as never);
        return 'failed';
      });
      renderPanel();
      await ready();
      fireEvent.change(textarea(), { target: { value: 'Why did it decline?' } });
      await act(async () => { fireEvent.submit(textarea().closest('form')!); });
      expect(draft()).toBe('');
    });

    it('reports a send that threw and restores the question', async () => {
      h.base.sendMessage.mockRejectedValueOnce(new Error('no model loaded'));
      renderPanel();
      await ready();
      fireEvent.change(textarea(), { target: { value: 'Why did it decline?' } });
      await act(async () => { fireEvent.submit(textarea().closest('form')!); });
      expect(h.toast.error).toHaveBeenCalledWith("Couldn't send in this tangent", { message: 'no model loaded' });
      expect(draft()).toBe('Why did it decline?');
    });

    it('sends nothing blank, nothing while unavailable, and only once at a time', async () => {
      const pending = deferred<string>();
      h.base.sendMessage.mockReturnValueOnce(pending.promise);
      const { rerender } = renderPanel({ unavailable: true });
      await ready();
      expect(screen.getByText('Choose an available chat model to reply.')).toBeInTheDocument();
      fireEvent.change(textarea(), { target: { value: 'Why?' } });
      fireEvent.submit(textarea().closest('form')!);
      expect(h.base.sendMessage).not.toHaveBeenCalled();
      rerender(
        <QueryClientProvider client={queryClient}><TooltipProvider>
          <TangentPanel tangent={tangent} toolPreferences={{ web: true } as never} unavailable={false} onPromoted={vi.fn()} />
        </TooltipProvider></QueryClientProvider>,
      );
      fireEvent.submit(textarea().closest('form')!);
      fireEvent.change(textarea(), { target: { value: 'Again?' } });
      fireEvent.submit(textarea().closest('form')!);
      expect(h.base.sendMessage).toHaveBeenCalledTimes(1);
      await act(async () => { pending.resolve('answered'); await pending.promise; });
      fireEvent.change(textarea(), { target: { value: '   ' } });
      fireEvent.submit(textarea().closest('form')!);
      expect(h.base.sendMessage).toHaveBeenCalledTimes(1);
    });

    it('stops a running reply', async () => {
      h.base.inFlightGenerations = new Map([['t1', 'r1']]);
      renderPanel();
      await ready();
      fireEvent.click(screen.getByRole('button', { name: 'Stop tangent response' }));
      expect(h.base.cancelGeneration).toHaveBeenCalledWith('t1');
      expect(screen.getByRole('button', { name: /Make conversation/ })).toBeDisabled();
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
  });

  describe('scope for messages', () => {
    it('regenerates with the tangent’s tools and restores the last question on failure', async () => {
      h.fetchMessages.mockResolvedValue([...context, message('m3', 'user', 'Which cities joined?'), message('m4', 'assistant', 'Lübeck.')]);
      renderPanel();
      await screen.findByText('Lübeck.');
      await act(async () => { await h.scope!.regenerateResponse('t1', { requestId: 'r' }); });
      expect(h.base.regenerateResponse).toHaveBeenCalledWith('t1', { requestId: 'r', toolPreferences: { web: true } });
      expect(draft()).toBe('');
      h.base.regenerateResponse.mockResolvedValueOnce('failed');
      await act(async () => { await h.scope!.regenerateResponse('t1', { toolPreferences: { web: false } }); });
      expect(h.base.regenerateResponse).toHaveBeenLastCalledWith('t1', { toolPreferences: { web: false } });
      expect(draft()).toBe('Which cities joined?');
    });

    it('writes edits to the tangent’s draft and scopes the conversation', async () => {
      renderPanel();
      await ready();
      expect(h.scope!.activeConversationId).toBe('t1');
      expect(h.scope!.conversations[0]).toMatchObject({ id: 't1', messages: context });
      act(() => h.scope!.setComposerDraft('Edited question'));
      expect(textarea()).toHaveValue('Edited question');
      act(() => h.scope!.setComposerDraft(null));
      expect(textarea()).toHaveValue('');
    });
  });

  it('follows new messages only while the reader is at the bottom', async () => {
    renderPanel();
    await ready();
    const transcript = screen.getByText(/Ask about this passage/).parentElement!;
    let top = 0;
    Object.defineProperties(transcript, {
      scrollHeight: { configurable: true, value: 1000 },
      clientHeight: { configurable: true, value: 300 },
      scrollTop: { configurable: true, get: () => top, set: (value: number) => { top = value; } },
    });
    top = 100;
    fireEvent.scroll(transcript);
    act(() => { queryClient.setQueryData(conversationKeys.messages('t1'), [...context, message('m3', 'user', 'First')]); });
    await screen.findByText('First');
    expect(top).toBe(100);
    top = 650;
    fireEvent.scroll(transcript);
    act(() => { queryClient.setQueryData(conversationKeys.messages('t1'), [...context, message('m3', 'user', 'First'), message('m4', 'assistant', 'Second')]); });
    await screen.findByText('Second');
    expect(top).toBe(1000);
  });
});
