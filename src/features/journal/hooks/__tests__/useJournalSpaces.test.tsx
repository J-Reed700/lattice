import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { ConversationJournalDto } from '@/types/api/conversation';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

import { useFollowNoteIdParam, useJournalSpaces } from '../useJournalSpaces';

const api = vi.hoisted(() => ({
  listJournals: vi.fn(),
  createJournal: vi.fn(),
  updateJournal: vi.fn(),
  deleteJournal: vi.fn(),
  listWorkspaceNotes: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ default: api, VaultAPI: api }));

const journal = (id: string, name: string, isArchived = false) => ({ id, name, isArchived } as ConversationJournalDto);
const ok = <T,>(data: T) => ({ ok: true as const, data });

type Options = Parameters<typeof useJournalSpaces>[0];

function setup(overrides: Partial<Options> = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const options: Options = {
    requestedJournalSpaceId: null, journalSpace: null, activeNote: null, updateNote: vi.fn(),
    searchParams: new URLSearchParams(), navigate: vi.fn(), notify: vi.fn(), ...overrides,
  };
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  const view = renderHook((props: Options) => useJournalSpaces(props), { wrapper, initialProps: options });
  return { ...options, ...view, client };
}

beforeEach(() => {
  for (const fn of Object.values(api)) fn.mockReset();
  localStorage.clear();
  api.listJournals.mockResolvedValue(ok([journal('garden', 'Garden'), journal('history', 'History')]));
  api.createJournal.mockResolvedValue(ok(journal('new-j', 'Journal 3')));
  api.updateJournal.mockResolvedValue(ok(journal('garden', 'Allotment')));
  api.deleteJournal.mockResolvedValue(ok(null));
});

describe('choosing the journal on arrival', () => {
  it('remembers a requested journal that exists', async () => {
    const s = setup({ requestedJournalSpaceId: 'history' });
    await waitFor(() => expect(localStorage.getItem('journal.lastSpaceId')).toBe('history'));
    expect(s.navigate).not.toHaveBeenCalled();
  });

  it('moves off a requested journal that is gone, to the first active one', async () => {
    api.listJournals.mockResolvedValue(ok([journal('old', 'Old', true), journal('history', 'History')]));
    const s = setup({ requestedJournalSpaceId: 'old', searchParams: new URLSearchParams('journalSpaceId=old&entryId=e1') });
    await waitFor(() => expect(s.navigate).toHaveBeenCalledWith('/journals?journalSpaceId=history', { replace: true }));
  });

  it('clears a requested journal when none is active', async () => {
    api.listJournals.mockResolvedValue(ok([journal('old', 'Old', true)]));
    const s = setup({ requestedJournalSpaceId: 'old', searchParams: new URLSearchParams('journalSpaceId=old') });
    await waitFor(() => expect(s.navigate).toHaveBeenCalledWith('/journals', { replace: true }));
  });

  it('keeps other parameters when clearing the journal', async () => {
    api.listJournals.mockResolvedValue(ok([]));
    const s = setup({ requestedJournalSpaceId: 'old', searchParams: new URLSearchParams('journalSpaceId=old&noteId=n1') });
    await waitFor(() => expect(s.navigate).toHaveBeenCalledWith('/journals?noteId=n1', { replace: true }));
  });

  it('opens the last-used journal, or the first', async () => {
    localStorage.setItem('journal.lastSpaceId', 'history');
    const s = setup();
    await waitFor(() => expect(s.navigate).toHaveBeenCalledWith('/journals?journalSpaceId=history', { replace: true }));
    localStorage.setItem('journal.lastSpaceId', 'deleted');
    const t = setup();
    await waitFor(() => expect(t.navigate).toHaveBeenCalledWith('/journals?journalSpaceId=garden', { replace: true }));
  });

  it('does nothing when every journal is archived', async () => {
    api.listJournals.mockResolvedValue(ok([journal('old', 'Old', true)]));
    const s = setup();
    await waitFor(() => expect(api.listJournals).toHaveBeenCalled());
    expect(s.navigate).not.toHaveBeenCalled();
    expect(api.createJournal).not.toHaveBeenCalled();
  });

  it('creates the first journal on first run, once', async () => {
    api.listJournals.mockResolvedValue(ok([]));
    api.createJournal.mockResolvedValue(ok(journal('first', 'Journal 1')));
    const s = setup();
    await waitFor(() => expect(s.navigate).toHaveBeenCalledWith('/journals?journalSpaceId=first', { replace: true }));
    expect(api.createJournal).toHaveBeenCalledTimes(1);
    expect(api.createJournal).toHaveBeenCalledWith(expect.objectContaining({ name: 'Journal 1', icon: '📓', accentColor: '#aa503d' }));
    expect(localStorage.getItem('journal.lastSpaceId')).toBe('first');

    // Back on Journal with no selection after the last journal was deleted: stays empty.
    vi.mocked(s.navigate).mockClear();
    s.rerender({ ...s, searchParams: new URLSearchParams('x=1') } as Options);
    await waitFor(() => expect(api.listJournals.mock.calls.length).toBeGreaterThan(1));
    expect(api.createJournal).toHaveBeenCalledTimes(1);
  });

  it('reports a first journal that could not be created, and retries on request', async () => {
    api.listJournals.mockResolvedValue(ok([]));
    api.createJournal.mockResolvedValueOnce({ ok: false, error: 'disk full' });
    const s = setup();
    await waitFor(() => expect(s.result.current.topLevelError).toBe('Failed to create initial journal: disk full'));
    api.createJournal.mockResolvedValue(ok(journal('first', 'Journal 1')));
    act(() => s.result.current.retryJournalLoad());
    await waitFor(() => expect(s.result.current.topLevelError).toBeNull());
    await waitFor(() => expect(s.navigate).toHaveBeenCalledWith('/journals?journalSpaceId=first', { replace: true }));
  });
});

describe('journal actions', () => {
  it('switches journal, dropping the open entry', () => {
    const s = setup({ requestedJournalSpaceId: 'garden', searchParams: new URLSearchParams('journalSpaceId=garden&entryId=e1') });
    act(() => s.result.current.handleSwitchJournal('history'));
    expect(s.navigate).toHaveBeenCalledWith('/journals?journalSpaceId=history');
  });

  it('creates a journal with the next free name and opens it', async () => {
    api.listJournals.mockResolvedValue(ok([journal('a', 'Journal 3'), journal('b', 'notes')]));
    const s = setup({ requestedJournalSpaceId: 'a' });
    await act(() => s.result.current.handleCreateJournal());
    expect(api.createJournal).toHaveBeenCalledWith(expect.objectContaining({ name: 'Journal 4' }));
    expect(s.navigate).toHaveBeenCalledWith('/journals?journalSpaceId=new-j');
    expect(s.notify).toHaveBeenCalledWith('success', 'Created "Journal 3".');
    expect(s.result.current.isCreatingJournal).toBe(false);
  });

  it('reports a journal that could not be created', async () => {
    const s = setup({ requestedJournalSpaceId: 'garden' });
    api.createJournal.mockResolvedValueOnce({ ok: false, error: 'name taken' });
    await act(() => s.result.current.handleCreateJournal());
    expect(s.notify).toHaveBeenCalledWith('error', 'Failed to create journal: name taken');
    api.listJournals.mockRejectedValueOnce(new Error('offline'));
    await act(() => s.result.current.handleCreateJournal());
    expect(s.notify).toHaveBeenCalledWith('error', expect.stringContaining('Failed to create journal:'));
  });

  it('renames the journal and its default page title with it', async () => {
    const note = { id: 'p1', title: 'Journal · Garden' } as WorkspaceNote;
    const s = setup({ requestedJournalSpaceId: 'garden', journalSpace: journal('garden', 'Garden'), activeNote: note });
    await act(() => s.result.current.handleRenameJournal('Allotment'));
    expect(api.updateJournal).toHaveBeenCalledWith({ journalId: 'garden', name: 'Allotment' });
    expect(s.notify).toHaveBeenCalledWith('success', 'Renamed journal to "Allotment".');
    const update = vi.mocked(s.updateNote).mock.calls[0][0];
    expect(update(note).title).toBe('Journal · Allotment');
  });

  it('leaves a page with its own title alone on rename, and reports a failed rename', async () => {
    const s = setup({ requestedJournalSpaceId: 'garden', journalSpace: journal('garden', 'Garden'), activeNote: { id: 'p1', title: 'Seeds' } as WorkspaceNote });
    await act(() => s.result.current.handleRenameJournal('Allotment'));
    expect(s.updateNote).not.toHaveBeenCalled();
    api.updateJournal.mockResolvedValueOnce({ ok: false, error: 'name taken' });
    await act(() => s.result.current.handleRenameJournal('History'));
    expect(s.notify).toHaveBeenCalledWith('error', 'name taken');
  });

  it('does not rename or delete without an open journal', async () => {
    const s = setup();
    await act(() => s.result.current.handleRenameJournal('x'));
    await act(() => s.result.current.handleDeleteJournal());
    expect(api.updateJournal).not.toHaveBeenCalled();
    expect(api.deleteJournal).not.toHaveBeenCalled();
  });

  it('deletes the journal, forgets its local state and opens the next one', async () => {
    localStorage.setItem('journal.lastSpaceId', 'garden');
    localStorage.setItem('journal.noteBySpace.garden', 'p1');
    const s = setup({ requestedJournalSpaceId: 'garden', journalSpace: journal('garden', 'Garden'), searchParams: new URLSearchParams('journalSpaceId=garden&entryId=e1') });
    api.listJournals.mockResolvedValue(ok([journal('history', 'History')]));
    await act(() => s.result.current.handleDeleteJournal());
    expect(api.deleteJournal).toHaveBeenCalledWith({ journalId: 'garden' });
    expect(localStorage.getItem('journal.lastSpaceId')).toBeNull();
    expect(localStorage.getItem('journal.noteBySpace.garden')).toBeNull();
    expect(s.navigate).toHaveBeenLastCalledWith('/journals?journalSpaceId=history', { replace: true });
    expect(s.notify).toHaveBeenCalledWith('success', 'Deleted journal "Garden".');
  });

  it('leaves the workspace empty after deleting the last journal, and reports a failed delete', async () => {
    const s = setup({ requestedJournalSpaceId: 'garden', journalSpace: journal('garden', 'Garden'), searchParams: new URLSearchParams('journalSpaceId=garden') });
    // Arriving on a journal remembers it; another journal remembered since is kept.
    await waitFor(() => expect(localStorage.getItem('journal.lastSpaceId')).toBe('garden'));
    localStorage.setItem('journal.lastSpaceId', 'history');
    api.listJournals.mockResolvedValue(ok([]));
    await act(() => s.result.current.handleDeleteJournal());
    expect(s.navigate).toHaveBeenLastCalledWith('/journals', { replace: true });
    expect(localStorage.getItem('journal.lastSpaceId')).toBe('history');

    api.deleteJournal.mockResolvedValueOnce({ ok: false, error: 'in use' });
    await act(() => s.result.current.handleDeleteJournal());
    expect(s.notify).toHaveBeenCalledWith('error', 'in use');
  });
});

describe('useFollowNoteIdParam', () => {
  function follow(noteId: string | null, search: string) {
    const options = { requestedNoteIdParam: noteId, searchParams: new URLSearchParams(search), setSearchParams: vi.fn(), setRequestedNoteId: vi.fn() };
    const view = renderHook(() => useFollowNoteIdParam(options));
    return { ...options, ...view };
  }

  it('hands the page over and moves to the journal that owns it', async () => {
    api.listWorkspaceNotes.mockResolvedValue(ok({ notes: [{ id: 'n1', journalId: 'history' }] }));
    const f = follow('n1', 'journalSpaceId=garden&entryId=e1&noteId=n1');
    await waitFor(() => expect(f.setSearchParams).toHaveBeenCalledWith(new URLSearchParams('journalSpaceId=history'), { replace: true }));
    expect(f.setRequestedNoteId).toHaveBeenCalledWith('n1');
  });

  it('stays in the journal when it already owns the page, or the owner is unknown', async () => {
    api.listWorkspaceNotes.mockResolvedValue(ok({ notes: [{ id: 'n1', journalId: 'garden' }] }));
    const same = follow('n1', 'journalSpaceId=garden&entryId=e1&noteId=n1');
    await waitFor(() => expect(same.setSearchParams).toHaveBeenCalledWith(new URLSearchParams('journalSpaceId=garden&entryId=e1'), { replace: true }));
    api.listWorkspaceNotes.mockResolvedValue({ ok: false, error: 'busy' });
    const unknown = follow('n2', 'noteId=n2');
    await waitFor(() => expect(unknown.setSearchParams).toHaveBeenCalledWith(new URLSearchParams(''), { replace: true }));
  });

  it('does nothing without a page, and ignores a reply after unmount', async () => {
    const none = follow(null, '');
    expect(api.listWorkspaceNotes).not.toHaveBeenCalled();
    none.unmount();

    let reply!: (value: unknown) => void;
    api.listWorkspaceNotes.mockReturnValue(new Promise((resolve) => { reply = resolve; }));
    const gone = follow('n1', 'noteId=n1');
    gone.unmount();
    await act(async () => { reply(ok({ notes: [] })); });
    expect(gone.setRequestedNoteId).not.toHaveBeenCalled();
  });
});
