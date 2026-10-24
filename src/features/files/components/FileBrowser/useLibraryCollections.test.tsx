import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { CustomCollection, DocumentMetadata, LibraryScope } from '@/types/fileBrowser';

import { useLibraryCollections } from './useLibraryCollections';

const h = vi.hoisted(() => ({
  actions: {
    create: vi.fn(),
    addDocuments: vi.fn(),
    removeDocuments: vi.fn(),
    deleteCollection: vi.fn(),
  },
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn() },
}));
vi.mock('@/features/files/hooks/useCustomCollectionsQuery', () => ({ useCustomCollectionActions: () => h.actions }));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));

const doc = (id: string) => ({ id, fileName: `${id}.md`, filePath: `/n/${id}.md`, category: 'document', fileType: 'md' }) as DocumentMetadata;
const documents = [doc('a'), doc('b'), doc('c')];
const collection = (id: string, name: string, kind: 'manual' | 'snapshot', documentIds: string[], parentId: string | null = null) =>
  ({ id, name, kind, documentIds, parentId, createdAt: '', updatedAt: '' }) as CustomCollection;
const collections = [
  collection('k1', 'Spices', 'manual', ['a', 'b']),
  collection('k2', 'Peppers', 'manual', ['c'], 'k1'),
  collection('k3', 'Snapshot: chili', 'snapshot', ['a']),
];

const setup = (opts: { scope?: LibraryScope; selected?: string[]; searchQuery?: string; customCollections?: CustomCollection[] } = {}) => {
  const fns = {
    setScope: vi.fn(),
    clearSelection: vi.fn(),
    setSearchQuery: vi.fn(),
    setFilterByType: vi.fn(),
    setFilterBySource: vi.fn(),
  };
  const hook = renderHook(() => useLibraryCollections({
    documents,
    customCollections: opts.customCollections ?? collections,
    scope: opts.scope ?? ({ kind: 'all' } as LibraryScope),
    selectedDocumentIds: new Set(opts.selected ?? []),
    searchQuery: opts.searchQuery ?? '',
    ...fns,
  }));
  return { ...hook, ...fns };
};

describe('useLibraryCollections', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.actions.create.mockResolvedValue('k9');
    h.actions.addDocuments.mockResolvedValue(undefined);
    h.actions.removeDocuments.mockResolvedValue(undefined);
    h.actions.deleteCollection.mockResolvedValue(undefined);
  });

  it('counts each collection including its children', () => {
    const { result } = setup();
    expect(result.current.collectionCounts.get('k1')).toBe(3);
    expect(result.current.collectionCounts.get('k2')).toBe(1);
    expect(result.current.activeCollection).toBeNull();
  });

  it('finds the active collection and the one documents are being added to', () => {
    const { result } = setup({ scope: { kind: 'collection', id: 'k2' } as LibraryScope });
    expect(result.current.activeCollection?.name).toBe('Peppers');
    act(() => result.current.setAddDocumentsCollectionId('k1'));
    expect(result.current.addDocumentsCollection?.name).toBe('Spices');
  });

  it('falls back to all documents when the scoped collection is gone', () => {
    const { setScope } = setup({ scope: { kind: 'collection', id: 'deleted' } as LibraryScope });
    expect(setScope).toHaveBeenCalledWith({ kind: 'all' });
  });

  describe('creating', () => {
    it('files the selection into a new collection and opens it, clearing filters', async () => {
      const { result, setScope, setSearchQuery, setFilterByType, setFilterBySource } = setup({ selected: ['a', 'c'] });
      await act(() => result.current.handleCreateCollection('Herbs'));
      expect(h.actions.create).toHaveBeenCalledWith('Herbs');
      expect(h.actions.addDocuments).toHaveBeenCalledWith('k9', ['a', 'c']);
      expect(setSearchQuery).toHaveBeenCalledWith('');
      expect(setFilterByType).toHaveBeenCalledWith(null);
      expect(setFilterBySource).toHaveBeenCalledWith('all');
      expect(setScope).toHaveBeenCalledWith({ kind: 'collection', id: 'k9' });
      expect(result.current.addDocumentsCollection).toBeUndefined();
    });

    it('asks for documents when nothing was selected', async () => {
      const { result } = setup({ customCollections: [...collections, collection('k9', 'Herbs', 'manual', [])] });
      await act(() => result.current.handleCreateCollection('Herbs'));
      expect(h.actions.addDocuments).not.toHaveBeenCalled();
      expect(result.current.addDocumentsCollection?.name).toBe('Herbs');
    });

    it('reports a duplicate name', async () => {
      h.actions.create.mockResolvedValueOnce(null);
      const { result, setScope } = setup();
      await act(() => result.current.handleCreateCollection('Spices'));
      expect(h.toast.warning).toHaveBeenCalledWith('Collection not created', { message: 'That name is already in use.' });
      expect(setScope).not.toHaveBeenCalled();
    });

    it('reports a collection that could not be saved', async () => {
      h.actions.create.mockRejectedValueOnce(new Error('disk full'));
      const { result } = setup();
      await act(() => result.current.handleCreateCollection('Herbs'));
      expect(h.toast.error).toHaveBeenCalledWith('Collection could not be saved', { message: 'disk full' });
      h.actions.create.mockRejectedValueOnce('nope');
      await act(() => result.current.handleCreateCollection('Herbs'));
      expect(h.toast.error).toHaveBeenLastCalledWith('Collection could not be saved', { message: 'nope' });
    });

    it('still opens the new collection when filing the documents fails', async () => {
      h.actions.addDocuments.mockRejectedValueOnce(new Error('one was deleted'));
      const { result, setScope } = setup({ selected: ['a'] });
      await act(() => result.current.handleCreateCollection('Herbs'));
      expect(h.toast.error).toHaveBeenCalledWith('Documents could not be added to the collection', { message: 'one was deleted' });
      expect(setScope).toHaveBeenCalledWith({ kind: 'collection', id: 'k9' });
      h.actions.addDocuments.mockRejectedValueOnce('x');
      await act(() => result.current.handleCreateCollection('Herbs'));
      expect(h.toast.error).toHaveBeenLastCalledWith('Documents could not be added to the collection', { message: 'x' });
    });
  });

  describe('removing from a collection', () => {
    it('removes documents from a manual collection and says they stay in the Library', async () => {
      const { result, clearSelection } = setup({ scope: { kind: 'collection', id: 'k1' } as LibraryScope });
      await act(() => result.current.handleRemoveFromCollection(['a']));
      expect(h.actions.removeDocuments).toHaveBeenCalledWith('k1', ['a']);
      expect(clearSelection).toHaveBeenCalled();
      expect(h.toast.success).toHaveBeenCalledWith('Removed from Spices', { message: 'Documents are still in your Library.' });
    });

    it('never removes from a snapshot or outside a collection', async () => {
      const snap = setup({ scope: { kind: 'collection', id: 'k3' } as LibraryScope });
      await act(() => snap.result.current.handleRemoveFromCollection(['a']));
      const all = setup();
      await act(() => all.result.current.handleRemoveFromCollection(['a']));
      expect(h.actions.removeDocuments).not.toHaveBeenCalled();
    });

    it('reports a failed removal and keeps the selection', async () => {
      h.actions.removeDocuments.mockRejectedValueOnce(new Error('locked'));
      const { result, clearSelection } = setup({ scope: { kind: 'collection', id: 'k1' } as LibraryScope });
      await act(() => result.current.handleRemoveFromCollection(['a']));
      expect(h.toast.error).toHaveBeenCalledWith('Documents could not be removed from the collection', { message: 'locked' });
      expect(clearSelection).not.toHaveBeenCalled();
      h.actions.removeDocuments.mockRejectedValueOnce('gone');
      await act(() => result.current.handleRemoveFromCollection(['a']));
      expect(h.toast.error).toHaveBeenLastCalledWith('Documents could not be removed from the collection', { message: 'gone' });
    });
  });

  describe('snapshots', () => {
    it('captures the selection under the search query', async () => {
      const { result, clearSelection } = setup({ selected: ['a', 'b'], searchQuery: '  chili  ' });
      await act(() => result.current.handleSnapshotSelection());
      expect(h.actions.create).toHaveBeenCalledWith('Snapshot: chili', null, ['a', 'b']);
      expect(h.toast.success).toHaveBeenCalledWith('2 documents captured');
      expect(clearSelection).toHaveBeenCalled();
    });

    it('labels a snapshot with the date when there is no query', async () => {
      const { result } = setup({ selected: ['a'] });
      await act(() => result.current.handleSnapshotSelection());
      expect(h.actions.create).toHaveBeenCalledWith(`Snapshot: ${new Date().toLocaleDateString()}`, null, ['a']);
    });

    it('does nothing with no selection', async () => {
      const { result } = setup();
      await act(() => result.current.handleSnapshotSelection());
      expect(h.actions.create).not.toHaveBeenCalled();
    });

    it('reports a duplicate or failed snapshot', async () => {
      const { result, clearSelection } = setup({ selected: ['a'] });
      h.actions.create.mockResolvedValueOnce(null);
      await act(() => result.current.handleSnapshotSelection());
      expect(h.toast.warning).toHaveBeenCalledWith('Snapshot not created', { message: 'That name is already in use.' });
      h.actions.create.mockRejectedValueOnce(new Error('disk full'));
      await act(() => result.current.handleSnapshotSelection());
      expect(h.toast.error).toHaveBeenCalledWith('Snapshot could not be saved', { message: 'disk full' });
      h.actions.create.mockRejectedValueOnce('x');
      await act(() => result.current.handleSnapshotSelection());
      expect(h.toast.error).toHaveBeenLastCalledWith('Snapshot could not be saved', { message: 'x' });
      expect(clearSelection).not.toHaveBeenCalled();
    });
  });

  describe('deleting', () => {
    it('deletes the pending collection and closes the confirm', async () => {
      const { result } = setup();
      act(() => result.current.setPendingDeleteCollection(collections[0]));
      await act(() => result.current.confirmDeleteCollection());
      expect(h.actions.deleteCollection).toHaveBeenCalledWith('k1');
      expect(result.current.pendingDeleteCollection).toBeNull();
    });

    it('does nothing with nothing pending', async () => {
      const { result } = setup();
      await act(() => result.current.confirmDeleteCollection());
      expect(h.actions.deleteCollection).not.toHaveBeenCalled();
    });

    it('reports a failed delete and keeps the confirm open', async () => {
      h.actions.deleteCollection.mockRejectedValueOnce(new Error('in use'));
      const { result } = setup();
      act(() => result.current.setPendingDeleteCollection(collections[0]));
      await act(() => result.current.confirmDeleteCollection());
      expect(h.toast.error).toHaveBeenCalledWith('Collection could not be deleted', { message: 'in use' });
      expect(result.current.pendingDeleteCollection).toBe(collections[0]);
      h.actions.deleteCollection.mockRejectedValueOnce('x');
      await act(() => result.current.confirmDeleteCollection());
      expect(h.toast.error).toHaveBeenLastCalledWith('Collection could not be deleted', { message: 'x' });
    });
  });

  it('holds dialog state for selection and rename', () => {
    const { result } = setup();
    act(() => {
      result.current.setCollectionSelection(['a']);
      result.current.setRenameCollection(collections[1]);
    });
    expect(result.current.collectionSelection).toEqual(['a']);
    expect(result.current.renameCollection).toBe(collections[1]);
  });
});
