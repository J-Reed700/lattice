import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { WorkspaceNote } from '@/types/api/dailyNotes';

import { useJournalNote } from '../useJournalNote';

const listWorkspaceNotes = vi.fn();
const createWorkspaceNote = vi.fn();
const updateWorkspaceNote = vi.fn();
const deleteWorkspaceNote = vi.fn();

vi.mock('@/lib/api', () => {
  const api = {
    listWorkspaceNotes: () => listWorkspaceNotes(),
    createWorkspaceNote: (...args: unknown[]) => createWorkspaceNote(...args),
    updateWorkspaceNote: (...args: unknown[]) => updateWorkspaceNote(...args),
    deleteWorkspaceNote: (...args: unknown[]) => deleteWorkspaceNote(...args),
  };
  return { VaultAPI: api, default: api };
});

function note(overrides: Partial<WorkspaceNote> = {}): WorkspaceNote {
  return {
    id: 'note_journal',
    title: 'Journal · Journal 1',
    content: 'today',
    linkedDocumentIds: [],
    linkedConversationIds: [],
    highlights: [],
    stickyNotes: [],
    conversationSnapshots: [],
    createdAt: '2026-09-01T09:00:00.000Z',
    updatedAt: '2026-09-06T09:00:00.000Z',
    ...overrides,
  } as WorkspaceNote;
}

const weekPage = note({
  id: 'note_week',
  title: 'Week of Sep 1',
  content: '## Journal Synthesis',
  updatedAt: '2026-09-06T18:00:00.000Z',
});

function mount(requestedNoteId: string | null = null) {
  return renderHook(() =>
    useJournalNote({
      journalSpaceId: 'space_1',
      journalName: 'Journal 1',
      requestedNoteId,
    }),
  );
}

describe('useJournalNote pages', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    listWorkspaceNotes.mockResolvedValue({ ok: true, data: { notes: [note(), weekPage] } });
    createWorkspaceNote.mockImplementation(async (title: string) => ({
      ok: true,
      data: note({ id: 'note_new', title, content: '', updatedAt: '2026-09-06T20:00:00.000Z' }),
    }));
    updateWorkspaceNote.mockImplementation(async (next: WorkspaceNote) => ({
      ok: true,
      data: { ...next, updatedAt: '2026-09-06T21:00:00.000Z' },
    }));
    deleteWorkspaceNote.mockResolvedValue({ ok: true, data: null });
  });

  it('lists every page, most recently updated first', async () => {
    const { result } = mount();
    await waitFor(() => expect(result.current.isLoadingNote).toBe(false));
    expect(result.current.pages.map((page) => page.id)).toEqual(['note_week', 'note_journal']);
  });

  it('opens this journal’s own page by default, not the newest one', async () => {
    const { result } = mount();
    await waitFor(() => expect(result.current.activeNote?.id).toBe('note_journal'));
  });

  it('opens the page a deep link asks for, and stops asking after that', async () => {
    const { result, rerender } = mount('note_week');
    await waitFor(() => expect(result.current.activeNote?.id).toBe('note_week'));
    expect(localStorage.getItem('journal.noteBySpace.space_1')).toBe('note_week');

    await act(async () => {
      await result.current.selectPage('note_journal');
    });
    expect(result.current.activeNote?.id).toBe('note_journal');

    // The same request must not drag the user back to the week page.
    rerender();
    await waitFor(() => expect(result.current.activeNote?.id).toBe('note_journal'));
  });

  it('remembers the last page it was on', async () => {
    localStorage.setItem('journal.noteBySpace.space_1', 'note_week');
    const { result } = mount();
    await waitFor(() => expect(result.current.activeNote?.id).toBe('note_week'));
  });

  it('flushes unsaved edits before leaving a page, and reads the next one fresh', async () => {
    const { result } = mount();
    await waitFor(() => expect(result.current.activeNote?.id).toBe('note_journal'));

    act(() => {
      result.current.updateNote((current) => ({ ...current, content: 'edited' }));
    });
    expect(result.current.hasPendingChanges).toBe(true);

    // The week page grew a synthesis while we were on another page.
    listWorkspaceNotes.mockResolvedValue({
      ok: true,
      data: { notes: [note(), { ...weekPage, content: '## Journal Synthesis\n\nnew block' }] },
    });

    await act(async () => {
      await result.current.selectPage('note_week');
    });

    expect(updateWorkspaceNote).toHaveBeenCalledWith(
      expect.objectContaining({ id: 'note_journal', content: 'edited' }),
    );
    expect(result.current.activeNote?.content).toBe('## Journal Synthesis\n\nnew block');
    expect(result.current.hasPendingChanges).toBe(false);
  });

  it('creates a page and opens it', async () => {
    const { result } = mount();
    await waitFor(() => expect(result.current.isLoadingNote).toBe(false));

    await act(async () => {
      await result.current.createPage('Page · Sat, Sep 6');
    });

    // A page is created owned by the journal it was created in, so deleting
    // that journal takes the page with it.
    expect(createWorkspaceNote).toHaveBeenCalledWith('Page · Sat, Sep 6', 'space_1');
    expect(result.current.activeNote?.id).toBe('note_new');
    expect(result.current.pages.map((page) => page.id)).toContain('note_new');
    expect(localStorage.getItem('journal.noteBySpace.space_1')).toBe('note_new');
  });

  it('picks up a page written elsewhere when the list is refreshed', async () => {
    const { result } = mount();
    await waitFor(() => expect(result.current.isLoadingNote).toBe(false));

    listWorkspaceNotes.mockResolvedValue({
      ok: true,
      data: {
        notes: [note(), weekPage, note({ id: 'note_capture', title: 'Daily Notes · Sep 6' })],
      },
    });
    await act(async () => {
      await result.current.refreshPages();
    });

    expect(result.current.pages.map((page) => page.id)).toContain('note_capture');
  });
  it('keeps newer typing while a slow save is pending and saves it next', async () => {
    const { result } = mount();
    await waitFor(() => expect(result.current.isLoadingNote).toBe(false));
    let finish!: (value: unknown) => void;
    updateWorkspaceNote.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
    act(() => result.current.updateNote((current) => ({ ...current, content: 'first draft' })));
    let saving!: Promise<boolean>;
    act(() => { saving = result.current.saveNow(); });
    await waitFor(() => expect(updateWorkspaceNote).toHaveBeenCalledTimes(1));
    act(() => result.current.updateNote((current) => ({ ...current, content: 'newer typing' })));
    await act(async () => {
      finish({ ok: true, data: note({ content: 'first draft' }) });
      await saving;
    });
    expect(result.current.activeNote?.content).toBe('newer typing');
    expect(updateWorkspaceNote).toHaveBeenLastCalledWith(expect.objectContaining({ content: 'newer typing' }));
    expect(result.current.hasPendingChanges).toBe(false);
  });

  it('keeps the current draft when saving fails during page navigation or creation', async () => {
    const { result } = mount();
    await waitFor(() => expect(result.current.isLoadingNote).toBe(false));
    updateWorkspaceNote.mockResolvedValue({ ok: false, error: 'Disk is full' });
    act(() => result.current.updateNote((current) => ({ ...current, content: 'unsaved work' })));
    await act(async () => { await result.current.selectPage('note_week'); });
    expect(result.current.activeNote?.content).toBe('unsaved work');
    expect(result.current.saveError).toBe('Disk is full');
    expect(result.current.hasPendingChanges).toBe(true);
    await act(async () => { expect(await result.current.createPage('New page')).toBeNull(); });
    expect(createWorkspaceNote).not.toHaveBeenCalled();
    expect(result.current.activeNote?.content).toBe('unsaved work');
  });

  it('opens the newest page rather than minting another when none carries the journal’s name', async () => {
    // Renaming the journal, or its first page, used to leave no title match —
    // and every such load added one more "Journal · …" page to the list.
    listWorkspaceNotes.mockResolvedValue({ ok: true, data: { notes: [weekPage] } });
    const { result } = mount();
    await waitFor(() => expect(result.current.activeNote?.id).toBe('note_week'));
    expect(createWorkspaceNote).not.toHaveBeenCalled();
  });

  it('starts an empty journal with one untitled page', async () => {
    listWorkspaceNotes.mockResolvedValue({ ok: true, data: { notes: [] } });
    const { result } = mount();
    await waitFor(() => expect(result.current.activeNote?.id).toBe('note_new'));
    expect(createWorkspaceNote).toHaveBeenCalledWith('Untitled page', 'space_1');
  });

  it('renames the open page through its own save, and another page directly', async () => {
    const { result } = mount();
    await waitFor(() => expect(result.current.isLoadingNote).toBe(false));

    await act(async () => { expect(await result.current.renamePage('note_journal', 'Field log')).toBe(true); });
    expect(result.current.activeNote?.title).toBe('Field log');
    expect(updateWorkspaceNote).toHaveBeenLastCalledWith(expect.objectContaining({ id: 'note_journal', title: 'Field log' }));

    await act(async () => { expect(await result.current.renamePage('note_week', 'Week one')).toBe(true); });
    expect(updateWorkspaceNote).toHaveBeenLastCalledWith(expect.objectContaining({ id: 'note_week', title: 'Week one' }));
    expect(result.current.pages.find((page) => page.id === 'note_week')?.title).toBe('Week one');
    // Renaming a page that is not open must not pull the reader onto it.
    expect(result.current.activeNote?.id).toBe('note_journal');
  });

  it('deletes a page, and opens the next one when it was the page being read', async () => {
    const { result } = mount();
    await waitFor(() => expect(result.current.activeNote?.id).toBe('note_journal'));

    await act(async () => { expect(await result.current.deletePage('note_journal')).toBe(true); });
    expect(deleteWorkspaceNote).toHaveBeenCalledWith('note_journal');
    expect(result.current.pages.map((page) => page.id)).toEqual(['note_week']);
    expect(result.current.activeNote?.id).toBe('note_week');
  });

  it('keeps the page when the delete is refused', async () => {
    deleteWorkspaceNote.mockResolvedValue({ ok: false, error: 'Locked' });
    const { result } = mount();
    await waitFor(() => expect(result.current.isLoadingNote).toBe(false));
    await act(async () => { expect(await result.current.deletePage('note_week')).toBe(false); });
    expect(result.current.pages).toHaveLength(2);
    expect(result.current.saveError).toBe('Locked');
  });

});
