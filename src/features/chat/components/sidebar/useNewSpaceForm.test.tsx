import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useNewSpaceForm } from './useNewSpaceForm';

const h = vi.hoisted(() => ({
  navigate: vi.fn(),
  setSelectedSpace: vi.fn(),
  loadConversations: vi.fn(),
  journals: [] as Array<{ id: string; name: string }>,
  createJournal: { mutateAsync: vi.fn(), isPending: false },
  toast: { success: vi.fn(), error: vi.fn() },
}));

vi.mock('react-router', () => ({ useNavigate: () => h.navigate }));
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: () => ({ setSelectedSpace: h.setSelectedSpace, loadConversations: h.loadConversations }),
}));
vi.mock('@/features/chat/hooks/workspaceQueries', () => ({
  useJournalsQuery: () => ({ journals: h.journals }),
  useCreateJournalMutation: () => h.createJournal,
}));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

const makeEditor = (overrides: Record<string, unknown> = {}) => ({
  createSpace: vi.fn().mockResolvedValue(true),
  isCreatingSpace: false,
  ...overrides,
}) as never;

describe('useNewSpaceForm', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.journals = [{ id: 'j1', name: 'Travel log' }];
    h.createJournal = { mutateAsync: vi.fn(), isPending: false };
    h.loadConversations.mockResolvedValue(undefined);
  });

  it('creates a standard space through the editor and resets on success', async () => {
    const editor = makeEditor();
    const { result } = renderHook(() => useNewSpaceForm(editor));
    act(() => {
      result.current.setIsCreateSpaceOpen(true);
      result.current.setNewSpaceNameDraft('Marine biology');
    });
    await act(() => result.current.createSpace());
    expect((editor as unknown as { createSpace: ReturnType<typeof vi.fn> }).createSpace).toHaveBeenCalledWith('Marine biology');
    expect(result.current.isCreateSpaceOpen).toBe(false);
    expect(result.current.newSpaceNameDraft).toBe('');
  });

  it('keeps the form open when the editor refuses', async () => {
    const editor = makeEditor({ createSpace: vi.fn().mockResolvedValue(false) });
    const { result } = renderHook(() => useNewSpaceForm(editor));
    act(() => {
      result.current.setIsCreateSpaceOpen(true);
      result.current.setNewSpaceNameDraft('Marine biology');
    });
    await act(() => result.current.createSpace());
    expect(result.current.isCreateSpaceOpen).toBe(true);
    expect(result.current.newSpaceNameDraft).toBe('Marine biology');
  });

  it('creates a journal with a unique name and opens it', async () => {
    h.createJournal.mutateAsync.mockResolvedValueOnce({ id: 'j 2', name: 'Travel log 2' });
    const { result } = renderHook(() => useNewSpaceForm(makeEditor()));
    act(() => {
      result.current.setIsCreateSpaceOpen(true);
      result.current.setNewSpaceKindDraft('journal');
      result.current.setNewSpaceNameDraft('travel log');
    });
    await act(() => result.current.createSpace());
    expect(h.createJournal.mutateAsync).toHaveBeenCalledWith(expect.objectContaining({ name: 'travel log 2', description: null }));
    expect(h.setSelectedSpace).toHaveBeenCalledWith(null);
    expect(h.loadConversations).toHaveBeenCalledWith({ spaceId: null });
    expect(h.navigate).toHaveBeenCalledWith('/journals?journalSpaceId=j%202&panel=entries');
    expect(h.toast.success).toHaveBeenCalledWith('Journal created', { message: 'travel log 2' });
    expect(result.current.isCreateSpaceOpen).toBe(false);
    expect(result.current.newSpaceKindDraft).toBe('standard');
  });

  it('reports a failed journal and keeps the form', async () => {
    h.createJournal.mutateAsync.mockRejectedValueOnce(new Error('name taken'));
    const { result } = renderHook(() => useNewSpaceForm(makeEditor()));
    act(() => {
      result.current.setIsCreateSpaceOpen(true);
      result.current.setNewSpaceKindDraft('journal');
    });
    await act(() => result.current.createSpace());
    expect(h.createJournal.mutateAsync).toHaveBeenCalledWith(expect.objectContaining({ name: 'Journal 2' }));
    expect(h.toast.error).toHaveBeenCalledWith('Could not update space', { message: 'name taken' });
    expect(h.navigate).not.toHaveBeenCalled();
    expect(result.current.isCreateSpaceOpen).toBe(true);
  });

  it('reports a non-Error failure as text', async () => {
    h.createJournal.mutateAsync.mockRejectedValueOnce('offline');
    const { result } = renderHook(() => useNewSpaceForm(makeEditor()));
    act(() => result.current.setNewSpaceKindDraft('journal'));
    await act(() => result.current.createSpace());
    expect(h.toast.error).toHaveBeenCalledWith('Could not update space', { message: 'offline' });
  });

  it('is busy while either kind is being created', () => {
    const { result, rerender } = renderHook(({ editor }) => useNewSpaceForm(editor), {
      initialProps: { editor: makeEditor({ isCreatingSpace: true }) },
    });
    expect(result.current.isCreatingSpace).toBe(true);
    rerender({ editor: makeEditor() });
    expect(result.current.isCreatingSpace).toBe(false);
    h.createJournal.isPending = true;
    rerender({ editor: makeEditor() });
    expect(result.current.isCreatingSpace).toBe(true);
  });
});
