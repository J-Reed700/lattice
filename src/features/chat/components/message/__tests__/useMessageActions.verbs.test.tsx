import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useMessageActions } from '@/features/chat/components/message/useMessageActions';
import type { DisplayMessage } from '@/types/conversation';

const h = vi.hoisted(() => ({
  state: {} as Record<string, any>,
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (selector: (state: unknown) => unknown) => selector(h.state),
}));
vi.mock('@/features/settings/hooks/useSettingsQuery', () => ({ useSettingsQuery: () => ({ data: undefined }) }));
vi.mock('@/features/model/hooks/useDownloadedModels', () => ({ useDownloadedModels: () => ({ downloadedModels: [] }) }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));
vi.mock('@/utils/conversationTitles', () => ({ createDefaultConversationTitle: () => 'New chat' }));

const answer = { id: 'm2', conversationId: 'c1', role: 'assistant', content: 'The   Silk Road\nlinked Chang’an and Rome.' } as DisplayMessage;

type Options = Partial<Parameters<typeof useMessageActions>[0]>;
const render = (options: Options = {}) => renderHook(() => useMessageActions({
  message: answer,
  messageId: 'm2',
  conversationId: 'c1',
  conversationSpaceId: 's-hist',
  previousMessageId: 'm1',
  activeConversationId: 'c1',
  isMessageBookmarked: false,
  normalizedMarkdownContent: 'The Silk Road linked Chang’an and Rome.',
  ...options,
})).result;

describe('useMessageActions verbs', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.state = {
      conversations: [],
      bookmarkMessage: vi.fn().mockResolvedValue(true),
      unbookmarkMessage: vi.fn().mockResolvedValue(true),
      deleteMessage: vi.fn().mockResolvedValue(undefined),
      regenerateResponse: vi.fn().mockResolvedValue('answered'),
      truncateAfter: vi.fn().mockResolvedValue(true),
      forkConversation: vi.fn().mockResolvedValue('b1'),
      sendMessage: vi.fn().mockResolvedValue('answered'),
      createConversation: vi.fn().mockResolvedValue('n1'),
      setConversationModelChoice: vi.fn(),
    };
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('copies the answer as markdown', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
    const result = render();
    await act(() => result.current.handleCopy());
    expect(writeText).toHaveBeenCalledWith('The Silk Road linked Chang’an and Rome.');
    expect(result.current.modelChoice).toEqual({ kind: 'default' });
  });

  describe('references', () => {
    it('saves a message titled by its compacted text, and says what that does', async () => {
      const result = render();
      await act(() => result.current.handleBookmarkToggle());
      expect(h.state.bookmarkMessage).toHaveBeenCalledWith('c1', 'm2', 'The Silk Road linked Chang’an and Rome.', null);
      expect(h.toast.success).toHaveBeenCalledWith('Saved', { message: 'Lattice will use this in future answers.' });
    });

    it('cuts a long title and stays quiet when the save fails', async () => {
      h.state.bookmarkMessage.mockResolvedValueOnce(false);
      const result = render({ message: { ...answer, content: 'x'.repeat(100) } as DisplayMessage });
      await act(() => result.current.handleBookmarkToggle());
      expect(h.state.bookmarkMessage).toHaveBeenCalledWith('c1', 'm2', `${'x'.repeat(80)}...`, null);
      expect(h.toast.success).not.toHaveBeenCalled();
    });

    it('saves a blank message with no title', async () => {
      const result = render({ message: { ...answer, content: '   ' } as DisplayMessage });
      await act(() => result.current.handleBookmarkToggle());
      expect(h.state.bookmarkMessage).toHaveBeenCalledWith('c1', 'm2', null, null);
    });

    it('removes a saved reference', async () => {
      const result = render({ isMessageBookmarked: true });
      await act(() => result.current.handleBookmarkToggle());
      expect(h.state.unbookmarkMessage).toHaveBeenCalledWith('c1', 'm2');
      expect(h.state.bookmarkMessage).not.toHaveBeenCalled();
    });

    it('does nothing without an open conversation or a saved message', async () => {
      const r1 = render({ activeConversationId: null });
      await act(() => r1.current.handleBookmarkToggle());
      const r2 = render({ messageId: undefined });
      await act(() => r2.current.handleBookmarkToggle());
      expect(h.state.bookmarkMessage).not.toHaveBeenCalled();
    });
  });

  describe('regenerate', () => {
    it.each([['answered'], ['cancelled']])('says nothing when it %s', async (outcome) => {
      h.state.regenerateResponse.mockResolvedValueOnce(outcome);
      const r3 = render();
      await act(() => r3.current.handleRegenerate());
      expect(h.toast.error).not.toHaveBeenCalled();
    });

    it.each([
      ['busy', 'This conversation is still answering.'],
      ['failed', 'Your question is back in the composer.'],
    ])('explains a %s regenerate', async (outcome, message) => {
      h.state.regenerateResponse.mockResolvedValueOnce(outcome);
      const r4 = render();
      await act(() => r4.current.handleRegenerate());
      expect(h.toast.error).toHaveBeenCalledWith("Couldn't regenerate", { message });
    });

    it('does nothing without a conversation', async () => {
      const r5 = render({ conversationId: undefined });
      await act(() => r5.current.handleRegenerate());
      const r6 = render({ conversationId: undefined });
      await act(() => r6.current.handleTryWithModel('m', 'Model'));
      expect(h.state.regenerateResponse).not.toHaveBeenCalled();
    });
  });

  describe('branching and editing', () => {
    it('branches at this message and says so', async () => {
      const result = render();
      await act(() => result.current.handleBranch());
      expect(h.state.forkConversation).toHaveBeenCalledWith('c1', 'm2');
      expect(h.toast.success).toHaveBeenCalledWith('Branched', { message: "You're in the new conversation." });
    });

    it('stays quiet when branching fails or cannot start', async () => {
      h.state.forkConversation.mockResolvedValueOnce(null);
      const r7 = render();
      await act(() => r7.current.handleBranch());
      const r8 = render({ messageId: undefined });
      await act(() => r8.current.handleBranch());
      expect(h.toast.success).not.toHaveBeenCalled();
      expect(h.state.forkConversation).toHaveBeenCalledTimes(1);
    });

    it('sends an edit in place, replacing later turns', async () => {
      const result = render();
      act(() => result.current.setIsEditing(true));
      await act(() => result.current.handleEditSend('When did the Silk Road decline?'));
      expect(result.current.isEditing).toBe(false);
      expect(h.state.truncateAfter).toHaveBeenCalledWith('c1', 'm2', true);
      expect(h.state.sendMessage).toHaveBeenCalledWith('When did the Silk Road decline?', 'c1');
    });

    it('sends nothing when the truncate fails or there is nothing to edit', async () => {
      h.state.truncateAfter.mockResolvedValueOnce(false);
      const r9 = render();
      await act(() => r9.current.handleEditSend('x'));
      const r10 = render({ messageId: undefined });
      await act(() => r10.current.handleEditSend('x'));
      expect(h.state.sendMessage).not.toHaveBeenCalled();
    });

    it('sends an edit as a branch from the previous turn', async () => {
      const r11 = render();
      await act(() => r11.current.handleEditSendAsBranch('Who used it most?'));
      expect(h.state.forkConversation).toHaveBeenCalledWith('c1', 'm1');
      expect(h.state.sendMessage).toHaveBeenCalledWith('Who used it most?', 'b1');
      expect(h.toast.success).toHaveBeenCalledWith('Branched', expect.anything());
    });

    it('starts a new chat in the same space when the first turn is edited as a branch', async () => {
      const r12 = render({ previousMessageId: undefined });
      await act(() => r12.current.handleEditSendAsBranch('Who used it most?'));
      expect(h.state.createConversation).toHaveBeenCalledWith('New chat', 's-hist');
      expect(h.state.sendMessage).toHaveBeenCalledWith('Who used it most?', 'n1');
    });

    it('stops when no branch could be made', async () => {
      h.state.forkConversation.mockResolvedValueOnce(null);
      const r13 = render();
      await act(() => r13.current.handleEditSendAsBranch('x'));
      const r14 = render({ conversationId: undefined });
      await act(() => r14.current.handleEditSendAsBranch('x'));
      expect(h.state.sendMessage).not.toHaveBeenCalled();
    });
  });

  describe('delete', () => {
    it('deletes only after confirmation', async () => {
      vi.spyOn(window, 'confirm').mockReturnValueOnce(false).mockReturnValueOnce(true);
      const result = render();
      await act(() => result.current.handleDeleteMessage());
      expect(h.state.deleteMessage).not.toHaveBeenCalled();
      await act(() => result.current.handleDeleteMessage());
      expect(h.state.deleteMessage).toHaveBeenCalledWith('c1', 'm2');
    });

    it('cannot delete a message that is not saved yet', async () => {
      const confirm = vi.spyOn(window, 'confirm');
      const r15 = render({ messageId: undefined });
      await act(() => r15.current.handleDeleteMessage());
      const r16 = render({ message: { tempId: 't', role: 'user', content: 'x' } as unknown as DisplayMessage });
      await act(() => r16.current.handleDeleteMessage());
      expect(confirm).not.toHaveBeenCalled();
    });
  });
});
