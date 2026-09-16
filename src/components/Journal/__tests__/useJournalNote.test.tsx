import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { WorkspaceNote } from '@/types/api/dailyNotes';

import { useJournalNote } from '../useJournalNote';

const listWorkspaceNotes = vi.fn();
const createWorkspaceNote = vi.fn();
const updateWorkspaceNote = vi.fn();

vi.mock('@/lib/api', () => {
  const api = {
    listWorkspaceNotes: () => listWorkspaceNotes(),
    createWorkspaceNote: (...args: unknown[]) => createWorkspaceNote(...args),
    updateWorkspaceNote: (...args: unknown[]) => updateWorkspaceNote(...args),
    deleteWorkspaceNote: (...args: unknown[]) => args,
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

    expect(createWorkspaceNote).toHaveBeenCalledWith('Page · Sat, Sep 6');
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

});
