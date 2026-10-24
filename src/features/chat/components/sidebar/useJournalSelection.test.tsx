import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { deferred } from '@/tests/deferred';

import { useJournalSelection } from './useJournalSelection';

const h = vi.hoisted(() => ({
  navigate: vi.fn(),
  store: {} as Record<string, unknown>,
  journals: [] as Array<{ id: string; name: string; isArchived?: boolean }>,
  createJournal: vi.fn(),
  addToJournal: vi.fn(),
  toast: { success: vi.fn(), warning: vi.fn(), error: vi.fn() },
}));

vi.mock('react-router', () => ({ useNavigate: () => h.navigate }));
vi.mock('@/shared/conversations/conversationsStore', () => ({ useConversationsStore: () => h.store }));
vi.mock('@/features/chat/hooks/workspaceQueries', () => ({
  useJournalsQuery: () => ({ journals: h.journals }),
  useCreateJournalMutation: () => ({ mutateAsync: h.createJournal }),
  useAddConversationsToJournalMutation: () => ({ mutateAsync: h.addToJournal }),
}));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

const conversations = [{ id: 'c1' }, { id: 'c2' }, { id: 'c3' }];

const setStore = (overrides: Record<string, unknown> = {}) => {
  h.store = {
    conversations,
    spaces: [{ id: 's1', name: 'Botany' }],
    filterMode: 'all',
    selectedSpaceId: null,
    activeConversationId: null,
    ...overrides,
  };
};

describe('useJournalSelection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.journals = [
      { id: 'j1', name: 'Garden diary' },
      { id: 'j2', name: 'Old recipes', isArchived: true },
      { id: 'j3', name: 'Field notes' },
    ];
    setStore();
  });

  it('starts out of selection mode with nothing chosen', () => {
    const { result } = renderHook(() => useJournalSelection());
    expect(result.current.isSelectionMode).toBe(false);
    expect(result.current.selectedConversationCount).toBe(0);
    expect(result.current.hasConversations).toBe(true);
    expect(result.current.areAllVisibleConversationsSelected).toBe(false);
    expect(result.current.bulkSpaceIdDraft).toBe('');
  });

  it('defaults the destination to the first live journal when selecting', () => {
    const { result } = renderHook(() => useJournalSelection());
    act(() => result.current.setIsSelectionMode(true));
    expect(result.current.bulkSpaceIdDraft).toBe('j1');
  });

  it('defaults the destination to the open journal when one is selected', () => {
    setStore({ selectedSpaceId: 'j3' });
    const { result } = renderHook(() => useJournalSelection());
    act(() => result.current.setIsSelectionMode(true));
    expect(result.current.bulkSpaceIdDraft).toBe('j3');
  });

  it('replaces a destination that is not a live journal', () => {
    const { result } = renderHook(() => useJournalSelection());
    act(() => result.current.setIsSelectionMode(true));
    act(() => result.current.setBulkSpaceIdDraft('j2'));
    expect(result.current.bulkSpaceIdDraft).toBe('j1');
  });

  it('toggles single conversations and select-all', () => {
    const { result } = renderHook(() => useJournalSelection());
    act(() => result.current.setIsSelectionMode(true));
    act(() => result.current.toggleConversationSelection('c2'));
    expect([...result.current.selectedConversationIds]).toEqual(['c2']);
    act(() => result.current.toggleConversationSelection('c2'));
    expect(result.current.selectedConversationCount).toBe(0);

    act(() => result.current.toggleSelectAllVisibleConversations());
    expect(result.current.selectedConversationCount).toBe(3);
    expect(result.current.areAllVisibleConversationsSelected).toBe(true);
    act(() => result.current.toggleSelectAllVisibleConversations());
    expect(result.current.selectedConversationCount).toBe(0);
  });

  it('does nothing on select-all with no conversations', () => {
    setStore({ conversations: [] });
    const { result } = renderHook(() => useJournalSelection());
    act(() => result.current.toggleSelectAllVisibleConversations());
    expect(result.current.selectedConversationCount).toBe(0);
    expect(result.current.hasConversations).toBe(false);
  });

  it('drops chosen conversations that leave the visible list', () => {
    const { result, rerender } = renderHook(() => useJournalSelection());
    act(() => result.current.setIsSelectionMode(true));
    act(() => result.current.toggleSelectAllVisibleConversations());
    setStore({ conversations: [{ id: 'c1' }] });
    rerender();
    expect([...result.current.selectedConversationIds]).toEqual(['c1']);
  });

  it('clears the selection when leaving selection mode', () => {
    const { result } = renderHook(() => useJournalSelection());
    act(() => result.current.setIsSelectionMode(true));
    act(() => result.current.toggleConversationSelection('c1'));
    act(() => result.current.setIsSelectionMode(false));
    expect(result.current.selectedConversationCount).toBe(0);
    expect(result.current.bulkSpaceIdDraft).toBe('');
  });

  it('leaves selection mode when the list switches to snippets', () => {
    const { result, rerender } = renderHook(() => useJournalSelection());
    act(() => result.current.setIsSelectionMode(true));
    setStore({ filterMode: 'snippets' });
    rerender();
    expect(result.current.isSelectionMode).toBe(false);
  });

  describe('adding to a journal', () => {
    it('warns when nothing is selected', async () => {
      const { result } = renderHook(() => useJournalSelection());
      act(() => result.current.setIsSelectionMode(true));
      await act(() => result.current.addSelectedConversationsToJournal());
      expect(h.toast.warning).toHaveBeenCalledWith('No conversations selected');
      expect(h.addToJournal).not.toHaveBeenCalled();
    });

    it('warns when there is no journal to add to', async () => {
      h.journals = [];
      const { result } = renderHook(() => useJournalSelection());
      act(() => result.current.setIsSelectionMode(true));
      act(() => result.current.toggleConversationSelection('c1'));
      await act(() => result.current.addSelectedConversationsToJournal());
      expect(h.toast.warning).toHaveBeenCalledWith('Choose a destination journal');
      expect(h.addToJournal).not.toHaveBeenCalled();
    });

    it('warns when the destination is not a live journal', async () => {
      const { result } = renderHook(() => useJournalSelection());
      act(() => result.current.toggleConversationSelection('c1'));
      act(() => result.current.setBulkSpaceIdDraft('s1'));
      await act(() => result.current.addSelectedConversationsToJournal());
      expect(h.toast.warning).toHaveBeenCalledWith('Choose a destination journal');
    });

    it('adds every selected conversation, then leaves selection mode', async () => {
      const pending = deferred<Array<{ conversationId: string; result: { ok: boolean } }>>();
      h.addToJournal.mockReturnValueOnce(pending.promise);
      const { result } = renderHook(() => useJournalSelection());
      act(() => result.current.setIsSelectionMode(true));
      act(() => result.current.toggleConversationSelection('c1'));
      act(() => result.current.toggleConversationSelection('c3'));

      let done!: Promise<void>;
      act(() => { done = result.current.addSelectedConversationsToJournal(); });
      expect(result.current.isBulkMoving).toBe(true);
      expect(h.addToJournal).toHaveBeenCalledWith({ ids: ['c1', 'c3'], journalId: 'j1' });

      await act(async () => {
        pending.resolve([{ conversationId: 'c1', result: { ok: true } }, { conversationId: 'c3', result: { ok: true } }]);
        await done;
      });
      expect(h.toast.success).toHaveBeenCalledWith('Added to journal', expect.objectContaining({
        message: '2 added to Garden diary. Conversations stay in their current space.',
      }));
      expect(result.current.isSelectionMode).toBe(false);
      expect(result.current.isBulkMoving).toBe(false);
    });

    it('keeps only the failures selected after a partial add', async () => {
      h.addToJournal.mockResolvedValueOnce([
        { conversationId: 'c1', result: { ok: true } },
        { conversationId: 'c2', result: { ok: false } },
      ]);
      const { result } = renderHook(() => useJournalSelection());
      act(() => result.current.setIsSelectionMode(true));
      act(() => result.current.toggleConversationSelection('c1'));
      act(() => result.current.toggleConversationSelection('c2'));
      await act(() => result.current.addSelectedConversationsToJournal());
      expect([...result.current.selectedConversationIds]).toEqual(['c2']);
      expect(h.toast.warning).toHaveBeenCalledWith('Some conversations were not added', expect.objectContaining({
        message: '1/2 added to Garden diary.',
      }));
      expect(result.current.isSelectionMode).toBe(true);
    });

    it('reports a failed add', async () => {
      h.addToJournal.mockRejectedValueOnce(new Error('journal locked'));
      const { result } = renderHook(() => useJournalSelection());
      act(() => result.current.setIsSelectionMode(true));
      act(() => result.current.toggleConversationSelection('c1'));
      await act(() => result.current.addSelectedConversationsToJournal());
      expect(h.toast.error).toHaveBeenCalledWith('Could not update journal', { message: 'journal locked' });
      expect(result.current.isBulkMoving).toBe(false);
    });

    it('reports a non-Error rejection as text', async () => {
      h.addToJournal.mockRejectedValueOnce('offline');
      const { result } = renderHook(() => useJournalSelection());
      act(() => result.current.setIsSelectionMode(true));
      act(() => result.current.toggleConversationSelection('c1'));
      await act(() => result.current.addSelectedConversationsToJournal());
      expect(h.toast.error).toHaveBeenCalledWith('Could not update journal', { message: 'offline' });
    });
  });

  describe('quick journal', () => {
    it('picks the next free name and targets the new journal', async () => {
      h.journals = [{ id: 'j1', name: 'Journal 2' }];
      h.createJournal.mockResolvedValueOnce({ id: 'j9', name: 'Journal 3' });
      const { result } = renderHook(() => useJournalSelection());
      await act(() => result.current.createQuickJournal());
      expect(h.createJournal).toHaveBeenCalledWith(expect.objectContaining({ name: 'Journal 3', description: null }));
      expect(result.current.bulkSpaceIdDraft).toBe('j9');
      expect(h.toast.success).toHaveBeenCalledWith('Journal created', expect.objectContaining({ message: 'Journal 3 is ready.' }));
      expect(result.current.isCreatingQuickJournal).toBe(false);
    });

    it('ignores a second request while one is in flight', async () => {
      const pending = deferred<{ id: string; name: string }>();
      h.createJournal.mockReturnValueOnce(pending.promise);
      const { result } = renderHook(() => useJournalSelection());
      let first!: Promise<void>;
      act(() => { first = result.current.createQuickJournal(); });
      expect(result.current.isCreatingQuickJournal).toBe(true);
      await act(() => result.current.createQuickJournal());
      expect(h.createJournal).toHaveBeenCalledTimes(1);
      await act(async () => {
        pending.resolve({ id: 'j4', name: 'Journal 4' });
        await first;
      });
    });

    it('reports a failed create', async () => {
      h.createJournal.mockRejectedValueOnce(new Error('disk full'));
      const { result } = renderHook(() => useJournalSelection());
      await act(() => result.current.createQuickJournal());
      expect(h.toast.error).toHaveBeenCalledWith('Could not update journal', { message: 'disk full' });
    });
  });

  describe('opening the notebook', () => {
    it('opens the chosen journal at the active conversation', () => {
      setStore({ activeConversationId: 'c2' });
      const { result } = renderHook(() => useJournalSelection());
      act(() => result.current.setIsSelectionMode(true));
      act(() => result.current.setBulkSpaceIdDraft('j3'));
      act(() => result.current.openSelectedJournalNotebook());
      expect(h.navigate).toHaveBeenCalledWith('/journals?journalSpaceId=j3&panel=entries&entryId=c2');
    });

    it('falls back to the first journal with no destination chosen', () => {
      const { result } = renderHook(() => useJournalSelection());
      act(() => result.current.openSelectedJournalNotebook());
      expect(h.navigate).toHaveBeenCalledWith('/journals?journalSpaceId=j1&panel=entries');
    });

    it('stays put when there is no journal', () => {
      h.journals = [];
      const { result } = renderHook(() => useJournalSelection());
      act(() => result.current.openSelectedJournalNotebook());
      expect(h.navigate).not.toHaveBeenCalled();
    });
  });
});
