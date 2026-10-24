import type { ReactNode } from 'react';

import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { GENERAL_SPACE_ID } from '@/features/spaces/model/spaces';
import { ConversationSnapshotProvider } from '@/shared/conversations/conversationsStore';
import type { ConversationsState } from '@/shared/conversations/conversationsStore.types';

import { useJournalEntryActions } from '../useJournalEntryActions';

const api = vi.hoisted(() => ({ addConversationToJournal: vi.fn(), renameConversation: vi.fn(), deleteConversation: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: api, VaultAPI: api }));

let createConversation: ReturnType<typeof vi.fn>;

function setup(journalId: string | null = 'journal-1') {
  const options = {
    requestedJournalSpaceId: journalId, navigate: vi.fn(), notify: vi.fn(), saveNow: vi.fn().mockResolvedValue(true),
    reloadEntries: vi.fn().mockResolvedValue(undefined), removeEntry: vi.fn(),
  };
  const wrapper = ({ children }: { children: ReactNode }) => (
    <ConversationSnapshotProvider value={{ createConversation } as unknown as ConversationsState}>{children}</ConversationSnapshotProvider>
  );
  const { result } = renderHook(() => useJournalEntryActions(options), { wrapper });
  return { ...options, actions: () => result.current };
}

beforeEach(() => {
  createConversation = vi.fn().mockResolvedValue('chat 9');
  for (const fn of Object.values(api)) fn.mockReset().mockResolvedValue({ ok: true, data: null });
});

describe('useJournalEntryActions', () => {
  it('starts a chat in General, files it under the journal, saves the page and opens it', async () => {
    const s = setup();
    await act(() => s.actions().handleNewEntry());
    expect(createConversation).toHaveBeenCalledWith(expect.any(String), GENERAL_SPACE_ID);
    expect(api.addConversationToJournal).toHaveBeenCalledWith({ journalSpaceId: 'journal-1', conversationId: 'chat 9' });
    expect(s.saveNow).toHaveBeenCalled();
    expect(s.navigate).toHaveBeenCalledWith('/chat?conversationId=chat%209');
  });

  it('searches the space the user picked', async () => {
    const s = setup();
    await act(() => s.actions().handleNewEntry('botany'));
    expect(createConversation).toHaveBeenCalledWith(expect.any(String), 'botany');
  });

  it('does nothing without a journal', async () => {
    const s = setup(null);
    await act(() => s.actions().handleNewEntry());
    expect(createConversation).not.toHaveBeenCalled();
  });

  it('stays put when the chat could not be filed or the page could not be saved', async () => {
    const s = setup();
    api.addConversationToJournal.mockResolvedValueOnce({ ok: false, error: 'journal archived' });
    await act(() => s.actions().handleNewEntry());
    expect(s.notify).toHaveBeenCalledWith('error', 'Failed to attach entry to journal: journal archived');
    s.saveNow.mockResolvedValueOnce(false);
    await act(() => s.actions().handleNewEntry());
    expect(s.navigate).not.toHaveBeenCalled();
  });

  it.each([
    [new Error('model not loaded'), 'model not loaded'],
    ['boom', 'Failed to start a conversation.'],
  ])('reports a chat that could not be started (%s)', async (failure, message) => {
    createConversation.mockRejectedValue(failure);
    const s = setup();
    await act(() => s.actions().handleNewEntry());
    expect(s.notify).toHaveBeenCalledWith('error', message);
  });

  it('renames an entry and reloads, or reports why not', async () => {
    const s = setup();
    await act(() => s.actions().handleRenameEntry('c1', 'Pickling'));
    expect(api.renameConversation).toHaveBeenCalledWith('c1', 'Pickling');
    expect(s.reloadEntries).toHaveBeenCalledTimes(1);
    api.renameConversation.mockResolvedValueOnce({ ok: false, error: 'locked' });
    await act(() => s.actions().handleRenameEntry('c1', 'Pickling'));
    expect(s.notify).toHaveBeenCalledWith('error', 'Failed to rename entry: locked');
    expect(s.reloadEntries).toHaveBeenCalledTimes(1);
  });

  it('deletes an entry from the list, or reports why not', async () => {
    const s = setup();
    await act(() => s.actions().handleDeleteEntry('c1'));
    expect(s.removeEntry).toHaveBeenCalledWith('c1');
    expect(s.notify).toHaveBeenCalledWith('success', 'Entry deleted.');
    api.deleteConversation.mockResolvedValueOnce({ ok: false, error: 'in use' });
    await act(() => s.actions().handleDeleteEntry('c2'));
    expect(s.notify).toHaveBeenCalledWith('error', 'Failed to delete entry: in use');
    expect(s.removeEntry).toHaveBeenCalledTimes(1);
  });
});
