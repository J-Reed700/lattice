import { createElement, type ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useJournalNote } from '@/features/journal/hooks/useJournalNote';
import type { WorkspaceNote } from '@/types/api/dailyNotes';

const api = vi.hoisted(() => ({
  listWorkspaceNotes: vi.fn(),
  createWorkspaceNote: vi.fn(),
  updateWorkspaceNote: vi.fn(),
  deleteWorkspaceNote: vi.fn(),
}));
vi.mock('@/lib/api', () => ({ VaultAPI: api, default: api }));

const page = (id: string, overrides: Partial<WorkspaceNote> = {}) => ({
  id, revision: 0, title: id, content: 'Knead well.', linkedDocumentIds: [], linkedConversationIds: [], highlights: [], stickyNotes: [],
  conversationSnapshots: [], createdAt: '2026-10-01T09:00:00.000Z', updatedAt: '2026-10-01T09:00:00.000Z', journalId: 'space_1', ...overrides,
} as WorkspaceNote);

function mount(journalSpaceId: string | null = 'space_1') {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return renderHook(() => useJournalNote({ journalSpaceId, journalName: 'Kitchen' }), {
    wrapper: ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client: queryClient }, children),
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  let stored = [page('bread', { updatedAt: '2026-10-05T09:00:00.000Z' }), page('soup')];
  api.listWorkspaceNotes.mockImplementation(async () => ({ ok: true, data: { notes: stored } }));
  api.updateWorkspaceNote.mockImplementation(async (next: WorkspaceNote) => ({ ok: true, data: { ...next, revision: next.revision + 1 } }));
  api.deleteWorkspaceNote.mockImplementation(async (id: string) => {
    stored = stored.filter((item) => item.id !== id);
    return { ok: true, data: null };
  });
});

describe('useJournalNote autosave', () => {
  it('saves a burst of edits once, after typing pauses', async () => {
    const { result } = mount();
    await waitFor(() => expect(result.current.activeNote?.id).toBe('bread'));
    act(() => result.current.updateNote((note) => ({ ...note, content: 'Knead' })));
    act(() => result.current.updateNote((note) => ({ ...note, content: 'Knead for ten minutes.' })));
    expect(result.current.hasPendingChanges).toBe(true);
    expect(result.current.activeNote?.content).toBe('Knead for ten minutes.');
    await waitFor(() => expect(api.updateWorkspaceNote).toHaveBeenCalledTimes(1));
    expect(api.updateWorkspaceNote.mock.calls[0][0]).toMatchObject({ id: 'bread', content: 'Knead for ten minutes.' });
    await waitFor(() => expect(result.current.hasPendingChanges).toBe(false));
  });

  it('opens no page without a journal', async () => {
    const { result } = mount(null);
    await waitFor(() => expect(result.current.isLoadingNote).toBe(false));
    expect(result.current.activeNote).toBeNull();
    act(() => result.current.updateNote((note) => ({ ...note, content: 'ignored' })));
    expect(result.current.hasPendingChanges).toBe(false);
  });

  it('deletes the open page and leaves none open', async () => {
    const { result } = mount();
    await waitFor(() => expect(result.current.activeNote?.id).toBe('bread'));
    let deleted = false;
    await act(async () => { deleted = await result.current.deleteActiveNote(); });
    expect(deleted).toBe(true);
    expect(api.deleteWorkspaceNote).toHaveBeenCalledWith('bread');
    expect(result.current.activeNote).toBeNull();
    expect(result.current.pages.map((p) => p.id)).toEqual(['soup']);
    await expect(result.current.deleteActiveNote()).resolves.toBe(false);
  });

  it('keeps the page and says why when the delete is refused', async () => {
    api.deleteWorkspaceNote.mockResolvedValueOnce({ ok: false, error: 'page is locked' });
    const { result } = mount();
    await waitFor(() => expect(result.current.activeNote?.id).toBe('bread'));
    let deleted = true;
    await act(async () => { deleted = await result.current.deleteActiveNote(); });
    expect(deleted).toBe(false);
    expect(result.current.saveError).toBe('page is locked');
    expect(result.current.activeNote?.id).toBe('bread');
  });

  it('reports a journal whose pages could not be read', async () => {
    api.listWorkspaceNotes.mockResolvedValue({ ok: false, error: 'database locked' });
    const { result } = mount();
    await waitFor(() => expect(result.current.loadError).toContain('database locked'));
    expect(result.current.isLoadingNote).toBe(false);
  });

  it('saves when the window hides with unsaved edits', async () => {
    const { result } = mount();
    await waitFor(() => expect(result.current.activeNote?.id).toBe('bread'));
    act(() => result.current.updateNote((note) => ({ ...note, content: 'Rest the dough.' })));
    const visibility = vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden');
    act(() => { document.dispatchEvent(new Event('visibilitychange')); });
    visibility.mockRestore();
    await waitFor(() => expect(api.updateWorkspaceNote).toHaveBeenCalledWith(expect.objectContaining({ content: 'Rest the dough.' })));
  });
});
