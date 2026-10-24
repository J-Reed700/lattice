import { QueryClient } from '@tanstack/react-query';
import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { conversationUiStore } from '@/features/chat/stores/conversationUiStore';
import { conversationKeys } from '@/shared/conversations/conversationKeys';
import { deferred } from '@/tests/deferred';

import { createConversationLifecycleRegistry } from '../lifecycleRegistry';
import { useConversationMessageActions } from '../useConversationMessageActions';

const h = vi.hoisted(() => ({
  api: { truncateConversationAfter: vi.fn(), deleteConversationMessage: vi.fn() },
  saveBookmark: vi.fn(),
  removeBookmark: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ VaultAPI: h.api, default: h.api }));
vi.mock('@/features/references/api/queries', () => ({ saveBookmark: h.saveBookmark, removeBookmark: h.removeBookmark }));

const message = (id: string, content: string) => ({ id, conversationId: 'c1', role: 'user', content, status: 'completed', createdAt: '' });

let queryClient: QueryClient;
let lifecycle: ReturnType<typeof createConversationLifecycleRegistry>;
const invalidateLists = vi.fn();

const setup = () => renderHook(() => useConversationMessageActions({ queryClient, lifecycle, invalidateLists })).result.current;
const uiError = () => conversationUiStore.getState().error;
const cached = () => (queryClient.getQueryData(conversationKeys.messages('c1')) as Array<{ id: string }> | undefined)?.map((item) => item.id);

describe('useConversationMessageActions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    queryClient = new QueryClient();
    lifecycle = createConversationLifecycleRegistry();
    invalidateLists.mockResolvedValue(undefined);
    conversationUiStore.setState({ error: null, optimisticMessages: new Map(), composerDraft: null } as never);
  });

  it('bookmarks and unbookmarks, reporting failures', async () => {
    const actions = setup();
    h.saveBookmark.mockResolvedValueOnce(undefined).mockRejectedValueOnce(new Error('bookmark table locked'));
    let outcome!: boolean;
    await act(async () => { outcome = await actions.bookmarkMessage('c1', 'm1', 'Carthage', 'note'); });
    expect(outcome).toBe(true);
    expect(h.saveBookmark).toHaveBeenCalledWith(queryClient, { conversationId: 'c1', messageId: 'm1', title: 'Carthage', note: 'note' });
    await act(async () => { outcome = await actions.bookmarkMessage('c1', 'm1'); });
    expect(outcome).toBe(false);
    expect(uiError()).toBe('bookmark table locked');
    h.removeBookmark.mockResolvedValueOnce(undefined).mockRejectedValueOnce('gone');
    await act(async () => { outcome = await actions.unbookmarkMessage('c1', 'm1'); });
    expect(outcome).toBe(true);
    await act(async () => { outcome = await actions.unbookmarkMessage('c1', 'm1'); });
    expect(outcome).toBe(false);
    expect(uiError()).toBe('gone');
  });

  describe('truncateAfter', () => {
    it('replaces the history with what survives and refreshes the lists', async () => {
      h.api.truncateConversationAfter.mockResolvedValueOnce({ ok: true, data: { conversationId: 'c1', deletedCount: 2, messages: [message('m1', 'Kept')] } });
      const actions = setup();
      let outcome!: boolean;
      await act(async () => { outcome = await actions.truncateAfter('c1', 'm1', true); });
      expect(outcome).toBe(true);
      expect(h.api.truncateConversationAfter).toHaveBeenCalledWith('c1', 'm1', true);
      expect(cached()).toEqual(['m1']);
      expect(invalidateLists).toHaveBeenCalled();
    });

    it('reports a refused truncate', async () => {
      h.api.truncateConversationAfter.mockResolvedValueOnce({ ok: false, error: 'message not found' });
      const actions = setup();
      let outcome!: boolean;
      await act(async () => { outcome = await actions.truncateAfter('c1', 'm1'); });
      expect(outcome).toBe(false);
      expect(h.api.truncateConversationAfter).toHaveBeenCalledWith('c1', 'm1', false);
      expect(uiError()).toBe('message not found');
    });

    it('drops a late answer for a conversation deleted meanwhile', async () => {
      const pending = deferred<unknown>();
      h.api.truncateConversationAfter.mockReturnValueOnce(pending.promise);
      const actions = setup();
      let run!: Promise<boolean>;
      act(() => { run = actions.truncateAfter('c1', 'm1'); });
      lifecycle.cleanupConversation('c1');
      let outcome!: boolean;
      await act(async () => { pending.resolve({ ok: true, data: { messages: [message('m1', 'x')] } }); outcome = await run; });
      expect(outcome).toBe(false);
      expect(cached()).toBeUndefined();
    });
  });

  describe('deleteMessage', () => {
    beforeEach(() => {
      queryClient.setQueryData(conversationKeys.messages('c1'), [message('m1', 'a'), message('m2', 'b'), message('m3', 'c')]);
    });

    it('removes the message at once and refreshes after the backend agrees', async () => {
      h.api.deleteConversationMessage.mockResolvedValueOnce({ ok: true, data: null });
      const actions = setup();
      await act(() => actions.deleteMessage('c1', 'm2'));
      expect(cached()).toEqual(['m1', 'm3']);
      expect(h.api.deleteConversationMessage).toHaveBeenCalledWith({ conversationId: 'c1', messageId: 'm2' });
      expect(invalidateLists).toHaveBeenCalled();
    });

    it('puts a refused deletion back in place', async () => {
      h.api.deleteConversationMessage.mockResolvedValueOnce({ ok: false, error: 'message locked' });
      const actions = setup();
      await act(() => actions.deleteMessage('c1', 'm2'));
      expect(cached()).toEqual(['m1', 'm2', 'm3']);
      expect(uiError()).toBe('message locked');
    });

    it('says when the message is not in the conversation', async () => {
      const actions = setup();
      await act(() => actions.deleteMessage('c1', 'm9'));
      expect(uiError()).toBe('Message not found in the selected conversation.');
      expect(h.api.deleteConversationMessage).not.toHaveBeenCalled();
    });

    it('ignores a late answer for a conversation deleted meanwhile', async () => {
      const pending = deferred<unknown>();
      h.api.deleteConversationMessage.mockReturnValueOnce(pending.promise);
      const actions = setup();
      let run!: Promise<void>;
      act(() => { run = actions.deleteMessage('c1', 'm2'); });
      lifecycle.cleanupConversation('c1');
      await act(async () => { pending.resolve({ ok: false, error: 'late' }); await run; });
      expect(uiError()).toBeNull();
      expect(cached()).toEqual(['m1', 'm3']);
    });
  });

  it('dismisses only a failed pending message', () => {
    conversationUiStore.setState({
      optimisticMessages: new Map([
        ['t1', { tempId: 't1', status: 'failed' }],
        ['t2', { tempId: 't2', status: 'pending' }],
      ]),
    } as never);
    const actions = setup();
    act(() => actions.dismissFailedMessage('t2'));
    act(() => actions.dismissFailedMessage('missing'));
    expect([...conversationUiStore.getState().optimisticMessages.keys()]).toEqual(['t1', 't2']);
    act(() => actions.dismissFailedMessage('t1'));
    expect([...conversationUiStore.getState().optimisticMessages.keys()]).toEqual(['t2']);
  });

  it('hands the composer a draft and clears errors', () => {
    conversationUiStore.setState({ error: 'old' } as never);
    const actions = setup();
    act(() => actions.setComposerDraft('Retry this question'));
    expect(conversationUiStore.getState().composerDraft).toBe('Retry this question');
    act(() => actions.clearError());
    expect(uiError()).toBeNull();
  });
});
