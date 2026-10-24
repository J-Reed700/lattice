import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { DocumentMetadata } from '@/types/fileBrowser';

import { useLibraryDocumentCommands } from './useLibraryDocumentCommands';

const h = vi.hoisted(() => ({
  toast: { success: vi.fn(), error: vi.fn() },
  spaces: { data: undefined as unknown[] | undefined, isLoading: false },
}));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));
vi.mock('@/features/spaces/api/queries', () => ({ useSpacesQuery: () => h.spaces }));

const doc = { id: 'd1', fileName: 'medieval-trade.pdf' } as DocumentMetadata;

const makeActions = () => ({
  reindex: { mutateAsync: vi.fn().mockResolvedValue(undefined) },
  removeFromIndex: { mutateAsync: vi.fn().mockResolvedValue(undefined) },
  addToSpace: { mutateAsync: vi.fn().mockResolvedValue(undefined), isPending: false },
});

const setup = (actions = makeActions(), selected = new Set<string>(['d1', 'd2'])) => {
  const clearSelection = vi.fn();
  const hook = renderHook(() => useLibraryDocumentCommands({
    documentActions: actions as never,
    selectedDocumentIds: selected,
    clearSelection,
  }));
  return { ...hook, actions, clearSelection };
};

describe('useLibraryDocumentCommands', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.spaces = {
      data: [
        { id: 's3', name: 'Zoology', isArchived: false },
        { id: 's1', name: 'Archived art', isArchived: true },
        { id: 's2', name: 'Botany', isArchived: false },
      ],
      isLoading: false,
    };
  });

  it('lists open spaces first, then archived, each by name', () => {
    const { result } = setup();
    expect(result.current.spaces.map((space) => space.name)).toEqual(['Botany', 'Zoology', 'Archived art']);
  });

  it('has no spaces before they load', () => {
    h.spaces = { data: undefined, isLoading: true };
    const { result } = setup();
    expect(result.current.spaces).toEqual([]);
    expect(result.current.isLoadingSpaces).toBe(true);
  });

  it('reindexes and reports either way', async () => {
    const { result, actions } = setup();
    await act(() => result.current.reindexDocument(doc));
    expect(actions.reindex.mutateAsync).toHaveBeenCalledWith(doc);
    expect(h.toast.success).toHaveBeenCalledWith('Reindexing medieval-trade.pdf');
    actions.reindex.mutateAsync.mockRejectedValueOnce(new Error('file moved'));
    await act(() => result.current.reindexDocument(doc));
    expect(h.toast.error).toHaveBeenCalledWith("Couldn't reindex medieval-trade.pdf", { message: 'file moved' });
  });

  it('removes from the index and reports either way', async () => {
    const { result, actions } = setup();
    await act(() => result.current.removeFromIndex(doc));
    expect(h.toast.success).toHaveBeenCalledWith('Removed medieval-trade.pdf from the index');
    actions.removeFromIndex.mutateAsync.mockRejectedValueOnce('locked');
    await act(() => result.current.removeFromIndex(doc));
    expect(h.toast.error).toHaveBeenCalledWith("Couldn't remove medieval-trade.pdf from the index", { message: 'locked' });
  });

  it('adds the selection to a space and clears it', async () => {
    const { result, actions, clearSelection } = setup();
    await act(() => result.current.handleBulkAssignToSpace('s2'));
    expect(actions.addToSpace.mutateAsync).toHaveBeenCalledWith({ documentIds: ['d1', 'd2'], spaceId: 's2' });
    expect(h.toast.success).toHaveBeenCalledWith('2 documents added to Botany');
    expect(clearSelection).toHaveBeenCalled();
  });

  it('names an unknown space generically', async () => {
    const { result } = setup(makeActions(), new Set(['d1']));
    await act(() => result.current.handleBulkAssignToSpace('gone'));
    expect(h.toast.success).toHaveBeenCalledWith('1 document added to space');
  });

  it('does nothing with an empty selection', async () => {
    const { result, actions } = setup(makeActions(), new Set());
    await act(() => result.current.handleBulkAssignToSpace('s2'));
    expect(actions.addToSpace.mutateAsync).not.toHaveBeenCalled();
  });

  it('reports a failed assignment and keeps the selection', async () => {
    const actions = makeActions();
    actions.addToSpace.mutateAsync.mockRejectedValueOnce(new Error('space archived'));
    const { result, clearSelection } = setup(actions);
    await act(() => result.current.handleBulkAssignToSpace('s2'));
    expect(h.toast.error).toHaveBeenCalledWith('Failed to add documents to space', { message: 'space archived' });
    expect(clearSelection).not.toHaveBeenCalled();
  });

  it('reflects a running assignment', () => {
    const actions = makeActions();
    actions.addToSpace.isPending = true;
    const { result } = setup(actions);
    expect(result.current.isAssigningSpace).toBe(true);
  });
});
