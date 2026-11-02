import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { speechPlayer } from '@/features/speech/model/speechPlayer';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { ConversationSnapshotProvider, useConversationsStore } from '@/shared/conversations/conversationsStore';
import { conversationDraftKey, useDraftStore } from '@/shared/drafts/drafts';

import { TangentConversation } from './TangentConversation';

const h = vi.hoisted(() => ({
  base: {} as Record<string, any>,
  scope: null as null | Record<string, any>,
  fetchMessages: vi.fn(),
  fetchConversationDetail: vi.fn(),
  fetchBookmarks: vi.fn(),
}));

vi.mock('@/features/chat/api/conversationQueryData', () => ({
  fetchMessages: h.fetchMessages,
  fetchConversationDetail: h.fetchConversationDetail,
  fetchBookmarks: h.fetchBookmarks,
}));

// What the chat inside a tangent sees of the shared controller.
function Probe() {
  h.scope = useConversationsStore() as unknown as Record<string, any>;
  return <p>Chat inside</p>;
}

const message = (id: string, role: 'user' | 'assistant', content: string, extra: Record<string, unknown> = {}) => (
  { id, conversationId: 't1', role, content, status: 'completed', createdAt: '2026-01-01T00:00:00Z', ...extra }
);
const inherited = [message('c1', 'user', 'Tell me about medieval trade'), message('c2', 'assistant', 'Trade routes grew…')];
const own = [message('m3', 'user', 'Which cities joined?'), message('m4', 'assistant', 'Lübeck led it.')];

let queryClient: QueryClient;
const tangent = () => <TangentConversation conversationId="t1" inheritedCount={2} onTurnEnd={h.base.onTurnEnd}><Probe /></TangentConversation>;
const wrapper = ({ children }: { children: ReactNode }) => (
  <QueryClientProvider client={queryClient}>
    <ConversationSnapshotProvider value={h.base as never}>{children}</ConversationSnapshotProvider>
  </QueryClientProvider>
);
const renderTangent = () => render(tangent(), { wrapper });
const ready = () => screen.findByText('Chat inside');
const draft = () => useDraftStore.getState().drafts[conversationDraftKey('t1')] ?? '';

describe('TangentConversation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    h.scope = null;
    h.base = {
      inFlightGenerations: new Map(),
      optimisticMessages: new Map(),
      regenerateResponse: vi.fn().mockResolvedValue('answered'),
      onTurnEnd: vi.fn(),
    };
    h.fetchMessages.mockResolvedValue([...inherited, ...own]);
    h.fetchConversationDetail.mockResolvedValue({ id: 't1', title: 'Hansa', spaceId: 'space-1' });
    h.fetchBookmarks.mockResolvedValue([]);
    conversationUiStore.setState({ optimisticMessages: new Map(), error: null } as never);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('opening', () => {
    it('says it is opening, and shows the chat once the tangent has loaded', async () => {
      renderTangent();
      expect(screen.getByRole('status')).toHaveTextContent('Opening tangent…');
      expect(screen.queryByText('Chat inside')).not.toBeInTheDocument();
      await ready();
      expect(screen.queryByRole('status')).not.toBeInTheDocument();
    });

    it('offers a retry when the tangent cannot be opened', async () => {
      h.fetchMessages.mockRejectedValueOnce(new Error('db locked'));
      renderTangent();
      expect(await screen.findByRole('alert')).toHaveTextContent('Couldn’t open this tangent.');
      expect(screen.queryByText('Chat inside')).not.toBeInTheDocument();
      fireEvent.click(screen.getByRole('button', { name: 'Try again' }));
      await ready();
      expect(h.fetchMessages).toHaveBeenCalledTimes(2);
    });

    it('treats a missing conversation as unopenable', async () => {
      h.fetchConversationDetail.mockResolvedValueOnce(null);
      renderTangent();
      expect(await screen.findByRole('alert')).toBeInTheDocument();
      expect(screen.queryByText('Chat inside')).not.toBeInTheDocument();
    });

    it('keeps the chat when a later refresh fails', async () => {
      renderTangent();
      await ready();
      h.fetchMessages.mockRejectedValue(new Error('offline'));
      await act(async () => { await queryClient.invalidateQueries({ queryKey: conversationKeys.messages('t1') }); });
      await waitFor(() => expect(h.fetchMessages).toHaveBeenCalledTimes(2));
      expect(screen.getByText('Chat inside')).toBeInTheDocument();
      expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    });
  });

  describe('what the chat sees', () => {
    it('is this conversation, without selecting it, after the context it inherited', async () => {
      renderTangent();
      await ready();
      expect(h.scope!.activeConversationId).toBe('t1');
      expect(h.scope!.conversations).toHaveLength(1);
      expect(h.scope!.conversations[0]).toMatchObject({ id: 't1', title: 'Hansa', spaceId: 'space-1', messages: own });
    });

    it('keeps the controller’s actions and its turns in flight', async () => {
      h.base.inFlightGenerations = new Map([['t1', 'r1']]);
      renderTangent();
      await ready();
      expect(h.scope!.inFlightGenerations).toBe(h.base.inFlightGenerations);
      expect(h.scope!.optimisticMessages).toBe(h.base.optimisticMessages);
    });

    it('carries the bookmarks and the answers’ evidence of this conversation', async () => {
      h.fetchBookmarks.mockResolvedValue([{ id: 'b1', conversationId: 't1', messageId: 'm4' }]);
      renderTangent();
      await ready();
      expect(h.scope!.messageBookmarks).toEqual([{ id: 'b1', conversationId: 't1', messageId: 'm4' }]);
      expect(h.scope!.messageBookmarkMap.get('m4')).toMatchObject({ id: 'b1' });
      expect(h.scope!.lastMessageSources).toBeInstanceOf(Map);
      expect(h.scope!.messageVerification).toBeInstanceOf(Map);
      expect(h.scope!.messageRetrieval).toBeInstanceOf(Map);
      expect(h.scope!.messageTurn).toBeInstanceOf(Map);
    });

    it('keeps the thread and the bookmarks steady while only the controller’s state moves', async () => {
      const view = renderTangent();
      await ready();
      const { conversations, messageBookmarkMap, lastMessageSources } = h.scope!;
      h.base = { ...h.base, inFlightGenerations: new Map([['elsewhere', 'r1']]) };
      view.rerender(tangent());
      await waitFor(() => expect(h.scope!.inFlightGenerations.has('elsewhere')).toBe(true));
      expect(h.scope!.conversations).toBe(conversations);
      expect(h.scope!.messageBookmarkMap).toBe(messageBookmarkMap);
      expect(h.scope!.lastMessageSources).toBe(lastMessageSources);
    });

    it('writes a draft handed to the composer into this tangent’s own draft', async () => {
      renderTangent();
      await ready();
      expect(h.scope!.composerDraft).toBeNull();
      act(() => h.scope!.setComposerDraft('Edited question'));
      expect(draft()).toBe('Edited question');
      act(() => h.scope!.setComposerDraft(null));
      expect(draft()).toBe('');
    });

    it('regenerates through the controller, and puts the last question back when that fails', async () => {
      renderTangent();
      await ready();
      await act(async () => { await h.scope!.regenerateResponse('t1', { model: 'qwen' }); });
      expect(h.base.regenerateResponse).toHaveBeenCalledWith('t1', { model: 'qwen' });
      expect(draft()).toBe('');
      h.base.regenerateResponse.mockResolvedValueOnce('failed');
      await act(async () => { await h.scope!.regenerateResponse('t1'); });
      expect(draft()).toBe('Which cities joined?');
    });
  });

  describe('failed questions', () => {
    it('forgets a failed question the backend turns out to have kept', async () => {
      const failed = { tempId: 'tmp', conversationId: 't1', role: 'user', content: 'Why?', status: 'failed', requestId: 'req-1' };
      conversationUiStore.setState({ optimisticMessages: new Map([['tmp', failed]]) } as never);
      h.fetchMessages.mockResolvedValue([...inherited, message('m5', 'user', 'Why?', { metadata: JSON.stringify({ requestId: 'req-1' }) })]);
      renderTangent();
      await ready();
      await waitFor(() => expect(conversationUiStore.getState().optimisticMessages.has('tmp')).toBe(false));
    });

    it('keeps one that never reached it', async () => {
      const failed = { tempId: 'tmp', conversationId: 't1', role: 'user', content: 'Why?', status: 'failed', requestId: 'req-1' };
      conversationUiStore.setState({ optimisticMessages: new Map([['tmp', failed]]) } as never);
      renderTangent();
      await ready();
      expect(conversationUiStore.getState().optimisticMessages.has('tmp')).toBe(true);
    });
  });

  describe('when a turn ends', () => {
    it('tells its owner once the turn that was running has ended', async () => {
      h.base.inFlightGenerations = new Map([['t1', 'r1']]);
      const view = renderTangent();
      await ready();
      expect(h.base.onTurnEnd).not.toHaveBeenCalled();
      h.base = { ...h.base, inFlightGenerations: new Map() };
      view.rerender(tangent());
      await waitFor(() => expect(h.base.onTurnEnd).toHaveBeenCalledTimes(1));
      view.rerender(tangent());
      expect(h.base.onTurnEnd).toHaveBeenCalledTimes(1);
    });

    it('says nothing when no turn ran, or when another conversation’s did', async () => {
      h.base.inFlightGenerations = new Map([['elsewhere', 'r1']]);
      const view = renderTangent();
      await ready();
      h.base = { ...h.base, inFlightGenerations: new Map() };
      view.rerender(tangent());
      expect(h.base.onTurnEnd).not.toHaveBeenCalled();
    });
  });

  it('stops reading aloud when the tangent goes', async () => {
    const stop = vi.spyOn(speechPlayer, 'stop');
    const { unmount } = renderTangent();
    await ready();
    stop.mockClear();
    unmount();
    expect(stop).toHaveBeenCalled();
  });
});
