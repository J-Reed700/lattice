/**
 * FileBrowserStore Regression Tests
 *
 * Tests for P0-3: Infinite Loop in FileBrowserStore
 * Ensures Zustand store works correctly without infinite re-renders
 */

import { renderHook, waitFor, act } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

import { selectFilteredDocuments, useFileBrowserStore } from './fileBrowserStore';
import _VaultAPI from '../lib/api';

// Mock API
vi.mock('../lib/api', () => ({
  default: {
    getRecentDocuments: vi.fn().mockResolvedValue({
      ok: true,
      data: [
        {
          id: '1',
          fileName: 'test.txt',
          filePath: '/test.txt',
          fileType: 'txt',
          category: 'document',
          language: 'en',
          modifiedAt: '2024-01-01',
          indexedAt: '2024-01-03',
          wordCount: 100,
        },
        {
          id: '2',
          fileName: 'document.pdf',
          filePath: '/document.pdf',
          fileType: 'pdf',
          category: 'document',
          language: 'en',
          modifiedAt: '2024-01-02',
          indexedAt: '2024-01-03',
          wordCount: 2000,
        },
      ],
    }),
    listAllDocuments: vi.fn().mockResolvedValue({
      ok: true,
      data: [
        {
          id: '1',
          fileName: 'test.txt',
          filePath: '/test.txt',
          fileType: 'txt',
          category: 'document',
          language: 'en',
          modifiedAt: '2024-01-01',
          indexedAt: '2024-01-03',
          wordCount: 100,
        },
        {
          id: '2',
          fileName: 'document.pdf',
          filePath: '/document.pdf',
          fileType: 'pdf',
          category: 'document',
          language: 'en',
          modifiedAt: '2024-01-02',
          indexedAt: '2024-01-03',
          wordCount: 2000,
        },
      ],
    }),
  },
}));

describe('FileBrowserStore - Zustand Implementation (P0-3 Regression)', () => {
  let renderCount = 0;

  beforeEach(() => {
    vi.clearAllMocks();
    renderCount = 0;
    window.localStorage.clear();
    // Reset store state between tests
    useFileBrowserStore.setState({
      viewMode: 'list',
      density: 'comfortable',
      groupByDate: true,
      selectedDocumentIds: new Set(),
      sortField: 'name',
      sortOrder: 'asc',
      searchQuery: '',
      filterByType: null,
      filterBySource: 'all',
      contentSearchMatches: new Set(),
      isContentSearchLoading: false,
      documents: [],
      customCollections: [],
      savedViews: [],
      activeSavedViewId: null,
      savedSearches: [],
      activeSavedSearchId: null,
      sourceConnections: [],
      isLoading: false,
      error: null,
      listColumns: {
        words: true,
        modified: true,
        type: true,
      },
      contextMenuPosition: null,
      contextMenuDocument: null,
    });
  });

  afterEach(() => {
    // Clean up store
    // useFileBrowserStore.getState() cleanup if needed
  });

  it('should not cause infinite re-renders when changing sort field', async () => {
    const { result } = renderHook(() => {
      renderCount++;
      return useFileBrowserStore();
    });

    // Load files initially
    await act(async () => {
      await result.current.loadFiles();
    });

    await waitFor(() => {
      expect(result.current.documents.length).toBe(2);
    });

    const initialRenderCount = renderCount;

    // Change sort field - this should NOT cause infinite renders
    act(() => {
      result.current.setSortField('size');
    });

    // Wait a bit to see if re-renders stabilize
    await new Promise((resolve) => setTimeout(resolve, 100));

    const finalRenderCount = renderCount;

    // Should have re-rendered once for the sort change, not infinitely
    // Allow a few extra renders for React's reconciliation
    expect(finalRenderCount - initialRenderCount).toBeLessThan(10);
  });

  it('should not cause infinite re-renders when changing sort order', async () => {
    const { result } = renderHook(() => {
      renderCount++;
      return useFileBrowserStore();
    });

    await act(async () => {
      await result.current.loadFiles();
    });

    await waitFor(() => {
      expect(result.current.documents.length).toBe(2);
    });

    const initialRenderCount = renderCount;

    // Change sort order
    act(() => {
      result.current.toggleSortOrder();
    });

    await new Promise((resolve) => setTimeout(resolve, 100));

    const finalRenderCount = renderCount;

    expect(finalRenderCount - initialRenderCount).toBeLessThan(10);
  });

  it('should refresh files when calling refreshFiles()', async () => {
    const { result } = renderHook(() => useFileBrowserStore());
    const { default: VaultAPI } = await import('../lib/api');

    await act(async () => {
      await result.current.loadFiles();
    });

    await waitFor(() => {
      expect(result.current.documents.length).toBe(2);
    });

    const initialCallCount = (VaultAPI.listAllDocuments as any).mock.calls.length;

    // Call refreshFiles
    await act(async () => {
      await result.current.refreshFiles();
    });

    // Should have made another API call
    expect((VaultAPI.listAllDocuments as any).mock.calls.length).toBe(initialCallCount + 1);
  });

  it('should not re-create refreshFiles when unrelated state changes', async () => {
    const { result } = renderHook(() => useFileBrowserStore());

    await act(async () => {
      await result.current.loadFiles();
    });

    const initialRefresh = result.current.refreshFiles;

    // Change view mode (unrelated to sorting)
    act(() => {
      result.current.setViewMode('grid');
    });

    // refreshFiles reference should remain stable
    expect(result.current.refreshFiles).toBe(initialRefresh);
  });

  it('should keep refreshFiles stable across state changes', async () => {
    const { result } = renderHook(() => useFileBrowserStore());

    await act(async () => {
      await result.current.loadFiles();
    });

    const initialRefresh = result.current.refreshFiles;

    // Change sort field
    act(() => {
      result.current.setSortField('modified');
    });

    // refreshFiles should remain stable (no hooks, no dependencies)
    expect(result.current.refreshFiles).toBe(initialRefresh);
  });

  it('should sort files correctly when loadFiles is called with different sort params', async () => {
    const { result } = renderHook(() => useFileBrowserStore());

    // Load with default sort (name, asc)
    await act(async () => {
      await result.current.loadFiles();
    });

    await waitFor(() => {
      expect(result.current.documents.length).toBe(2);
    });

    // First file should be 'document.pdf' (alphabetically first)
    expect(result.current.documents[0].fileName).toBe('document.pdf');

    // Change to sort by size
    act(() => {
      result.current.setSortField('size');
    });

    await waitFor(() => {
      // Smaller file should be first
      expect(result.current.documents[0].fileName).toBe('test.txt');
      expect(result.current.documents[1].fileName).toBe('document.pdf');
    });
  });

  it('should not call loadFiles multiple times when dependencies change rapidly', async () => {
    const { result } = renderHook(() => useFileBrowserStore());
    const { default: VaultAPI } = await import('../lib/api');

    await act(async () => {
      await result.current.loadFiles();
    });

    const initialCallCount = (VaultAPI.listAllDocuments as any).mock.calls.length;

    // Rapidly change sort params
    act(() => {
      result.current.setSortField('size');
      result.current.setSortOrder('desc');
      result.current.setSortField('modified');
    });

    // Wait for any pending updates
    await new Promise((resolve) => setTimeout(resolve, 200));

    // Should NOT have triggered loadFiles automatically
    // (loadFiles should only be called manually, not as an effect)
    expect((VaultAPI.listAllDocuments as any).mock.calls.length).toBe(initialCallCount);
  });

  it('should handle concurrent loadFiles calls gracefully', async () => {
    const { result } = renderHook(() => useFileBrowserStore());

    // Call loadFiles multiple times concurrently
    await act(async () => {
      const promises = [
        result.current.loadFiles(),
        result.current.loadFiles(),
        result.current.loadFiles(),
      ];
      await Promise.all(promises);
    });

    // Should have loaded files successfully
    await waitFor(() => {
      expect(result.current.documents.length).toBe(2);
      expect(result.current.isLoading).toBe(false);
      expect(result.current.error).toBeNull();
    });
  });

  it('should create custom collection, dedupe names, and add/remove documents', () => {
    renderHook(() => useFileBrowserStore());

    let createdId: string | null = null;
    act(() => {
      createdId = useFileBrowserStore.getState().createCustomCollection(' Product Docs ');
    });
    expect(createdId).toBeTruthy();
    expect(useFileBrowserStore.getState().customCollections).toHaveLength(1);
    expect(useFileBrowserStore.getState().customCollections[0].name).toBe('Product Docs');
    expect(useFileBrowserStore.getState().customCollections[0].kind).toBe('manual');
    expect(useFileBrowserStore.getState().customCollections[0].parentId).toBeNull();

    let duplicateId: string | null = null;
    act(() => {
      duplicateId = useFileBrowserStore.getState().createCustomCollection('product docs');
    });
    expect(duplicateId).toBeNull();
    expect(useFileBrowserStore.getState().customCollections).toHaveLength(1);

    act(() => {
      useFileBrowserStore.getState().addDocumentsToCustomCollection(createdId!, ['a', 'b', 'a']);
    });
    expect(useFileBrowserStore.getState().customCollections[0].documentIds).toEqual(['a', 'b']);

    act(() => {
      useFileBrowserStore.getState().removeDocumentsFromCustomCollection(createdId!, ['a']);
    });
    expect(useFileBrowserStore.getState().customCollections[0].documentIds).toEqual(['b']);
  });

  it('should create immutable snapshot collection from document ids', () => {
    renderHook(() => useFileBrowserStore());

    let snapshotId: string | null = null;
    act(() => {
      snapshotId = useFileBrowserStore.getState().createSnapshotCollection('Search Snapshot', ['x', 'y', 'x']);
    });

    expect(snapshotId).toBeTruthy();
    const snapshot = useFileBrowserStore.getState().customCollections.find((collection) => collection.id === snapshotId);
    expect(snapshot).toBeTruthy();
    expect(snapshot?.kind).toBe('snapshot');
    expect(snapshot?.documentIds).toEqual(['x', 'y']);

    act(() => {
      useFileBrowserStore.getState().addDocumentsToCustomCollection(snapshotId!, ['z']);
      useFileBrowserStore.getState().removeDocumentsFromCustomCollection(snapshotId!, ['x']);
    });

    const unchanged = useFileBrowserStore.getState().customCollections.find((collection) => collection.id === snapshotId);
    expect(unchanged?.documentIds).toEqual(['x', 'y']);
  });

  it('should support nested custom collections and cascade delete descendants', () => {
    renderHook(() => useFileBrowserStore());

    let parentId = '';
    let childId = '';
    let grandchildId = '';

    act(() => {
      parentId = useFileBrowserStore.getState().createCustomCollection('Projects') ?? '';
      childId = useFileBrowserStore.getState().createCustomCollection('Roadmap', parentId) ?? '';
      grandchildId = useFileBrowserStore.getState().createCustomCollection('Q1', childId) ?? '';
    });

    const created = useFileBrowserStore.getState().customCollections;
    expect(created.find(collection => collection.id === parentId)?.parentId).toBeNull();
    expect(created.find(collection => collection.id === childId)?.parentId).toBe(parentId);
    expect(created.find(collection => collection.id === grandchildId)?.parentId).toBe(childId);

    act(() => {
      useFileBrowserStore.getState().deleteCustomCollection(parentId);
    });

    const remaining = useFileBrowserStore.getState().customCollections;
    expect(remaining.find(collection => collection.id === parentId)).toBeUndefined();
    expect(remaining.find(collection => collection.id === childId)).toBeUndefined();
    expect(remaining.find(collection => collection.id === grandchildId)).toBeUndefined();
  });

  it('should update custom collection name and parent with validation', () => {
    renderHook(() => useFileBrowserStore());

    let rootA = '';
    let rootB = '';
    let child = '';
    let grandchild = '';

    act(() => {
      rootA = useFileBrowserStore.getState().createCustomCollection('Root A') ?? '';
      rootB = useFileBrowserStore.getState().createCustomCollection('Root B') ?? '';
      child = useFileBrowserStore.getState().createCustomCollection('Child', rootA) ?? '';
      grandchild = useFileBrowserStore.getState().createCustomCollection('Grandchild', child) ?? '';
    });

    let updated = false;
    act(() => {
      updated = useFileBrowserStore
        .getState()
        .updateCustomCollection(child, { name: 'Product Roadmap', parentId: rootB });
    });
    expect(updated).toBe(true);

    const movedChild = useFileBrowserStore.getState().customCollections.find((collection) => collection.id === child);
    expect(movedChild?.name).toBe('Product Roadmap');
    expect(movedChild?.parentId).toBe(rootB);

    let preventedCycle = true;
    act(() => {
      preventedCycle = useFileBrowserStore
        .getState()
        .updateCustomCollection(rootB, { parentId: grandchild });
    });
    expect(preventedCycle).toBe(false);

    const unchangedRootB = useFileBrowserStore
      .getState()
      .customCollections.find((collection) => collection.id === rootB);
    expect(unchangedRootB?.parentId).toBeNull();
  });

  it('should reconcile local sources while preserving cloud sources', () => {
    renderHook(() => useFileBrowserStore());

    let cloudId = '';
    act(() => {
      cloudId = useFileBrowserStore.getState().createSourceConnection({
        name: 'Cloud Docs',
        provider: 'dropbox',
        mode: 'managed',
        health: 'attention',
        enabled: false,
        lastSyncedAt: null,
      });
    });

    act(() => {
      useFileBrowserStore.getState().reconcileLocalSources([
        {
          id: 'local:/Users/me/Documents',
          name: 'Documents',
          provider: 'local_folder',
          mode: 'referenced',
          health: 'healthy',
          enabled: true,
          path: '/Users/me/Documents',
          lastSyncedAt: '2026-02-07T00:00:00.000Z',
          createdAt: '2026-02-07T00:00:00.000Z',
          updatedAt: '2026-02-07T00:00:00.000Z',
        },
      ]);
    });

    expect(useFileBrowserStore.getState().sourceConnections.some(connection => connection.id === cloudId)).toBe(true);
    const local = useFileBrowserStore.getState().sourceConnections.find(connection => connection.id === 'local:/Users/me/Documents');
    expect(local).toBeTruthy();
    expect(local?.provider).toBe('local_folder');

    act(() => {
      useFileBrowserStore.getState().updateSourceConnection(cloudId, { enabled: true, health: 'healthy' });
    });
    const cloud = useFileBrowserStore.getState().sourceConnections.find(connection => connection.id === cloudId);
    expect(cloud?.enabled).toBe(true);
    expect(cloud?.health).toBe('healthy');
  });

  it('should clear stale content matches when search query changes', () => {
    renderHook(() => useFileBrowserStore());

    act(() => {
      useFileBrowserStore.setState({
        documents: [
          {
            id: '1',
            fileName: 'alpha.txt',
            filePath: '/alpha.txt',
            fileType: 'txt',
            category: 'document',
            language: 'en',
            modifiedAt: '2026-01-01',
            indexedAt: '2026-01-01',
            wordCount: 10,
          },
        ],
      });
      useFileBrowserStore.getState().setContentSearchMatches(['1']);
      useFileBrowserStore.getState().setSearchQuery('beta');
    });

    expect(useFileBrowserStore.getState().contentSearchMatches.size).toBe(0);
  });

  it('should apply saved view search without leaking stale content-match state', () => {
    renderHook(() => useFileBrowserStore());

    act(() => {
      useFileBrowserStore.setState({
        documents: [
          {
            id: '1',
            fileName: 'alpha.txt',
            filePath: '/alpha.txt',
            fileType: 'txt',
            category: 'document',
            language: 'en',
            modifiedAt: '2026-01-01',
            indexedAt: '2026-01-01',
            wordCount: 10,
          },
          {
            id: '2',
            fileName: 'gamma.txt',
            filePath: '/gamma.txt',
            fileType: 'txt',
            category: 'document',
            language: 'en',
            modifiedAt: '2026-01-01',
            indexedAt: '2026-01-01',
            wordCount: 10,
          },
        ],
      });
      useFileBrowserStore.getState().setSearchQuery('beta');
    });

    let savedViewId = '';
    act(() => {
      savedViewId = useFileBrowserStore.getState().createSavedView('Beta View') ?? '';
    });
    expect(savedViewId).not.toBe('');

    act(() => {
      useFileBrowserStore.getState().setContentSearchMatches(['1', '2']);
      useFileBrowserStore.getState().applySavedView(savedViewId);
    });

    const state = useFileBrowserStore.getState();
    expect(state.searchQuery).toBe('beta');
    expect(state.contentSearchMatches.size).toBe(0);
    expect(selectFilteredDocuments(state)).toHaveLength(0);
  });

  it('should scope saved view results to its base collection', () => {
    renderHook(() => useFileBrowserStore());

    act(() => {
      useFileBrowserStore.setState({
        documents: [
          {
            id: '1',
            fileName: 'alpha.txt',
            filePath: '/alpha.txt',
            fileType: 'txt',
            category: 'document',
            language: 'en',
            modifiedAt: '2026-01-01',
            indexedAt: '2026-01-01',
            wordCount: 10,
          },
          {
            id: '2',
            fileName: 'beta.txt',
            filePath: '/beta.txt',
            fileType: 'txt',
            category: 'document',
            language: 'en',
            modifiedAt: '2026-01-01',
            indexedAt: '2026-01-01',
            wordCount: 10,
          },
        ],
      });
    });

    let collectionId = '';
    let viewId = '';
    act(() => {
      collectionId =
        useFileBrowserStore.getState().createSnapshotCollection('Only Alpha', ['1']) ?? '';
      viewId = useFileBrowserStore.getState().createSavedView('Scoped View') ?? '';
      useFileBrowserStore.getState().updateSavedView(viewId, { baseCollectionId: collectionId });
      useFileBrowserStore.getState().applySavedView(viewId);
    });

    const state = useFileBrowserStore.getState();
    const filtered = selectFilteredDocuments(state);
    expect(filtered).toHaveLength(1);
    expect(filtered[0]?.id).toBe('1');
  });

  it('should persist list column visibility in saved views', () => {
    renderHook(() => useFileBrowserStore());

    let viewId = '';
    act(() => {
      useFileBrowserStore.getState().setListColumnVisibility({
        words: false,
        modified: true,
        type: false,
      });
      viewId = useFileBrowserStore.getState().createSavedView('List Columns View') ?? '';
      useFileBrowserStore.getState().setListColumnVisibility({
        words: true,
        modified: false,
        type: true,
      });
      useFileBrowserStore.getState().applySavedView(viewId);
    });

    const state = useFileBrowserStore.getState();
    expect(viewId).not.toBe('');
    expect(state.listColumns).toEqual({
      words: false,
      modified: true,
      type: false,
    });
  });

  it('should create and apply saved searches', () => {
    renderHook(() => useFileBrowserStore());

    let searchId = '';
    act(() => {
      useFileBrowserStore.getState().setSearchQuery('anthocyanin');
      useFileBrowserStore.getState().setFilterBySource('web');
      searchId = useFileBrowserStore.getState().createSavedSearch('Anthocyanin search') ?? '';
      useFileBrowserStore.getState().setSearchQuery('override');
      useFileBrowserStore.getState().setFilterBySource('all');
      useFileBrowserStore.getState().applySavedSearch(searchId);
    });

    const state = useFileBrowserStore.getState();
    expect(searchId).not.toBe('');
    expect(state.searchQuery).toBe('anthocyanin');
    expect(state.filterBySource).toBe('web');
    expect(state.activeSavedSearchId).toBe(searchId);
  });

  it('should reorder saved searches deterministically', () => {
    renderHook(() => useFileBrowserStore());

    let firstId = '';
    let secondId = '';
    act(() => {
      useFileBrowserStore.getState().setSearchQuery('first');
      firstId = useFileBrowserStore.getState().createSavedSearch('First') ?? '';
      useFileBrowserStore.getState().setSearchQuery('second');
      secondId = useFileBrowserStore.getState().createSavedSearch('Second') ?? '';
      useFileBrowserStore.getState().reorderSavedSearches([firstId, secondId]);
    });

    const ids = useFileBrowserStore.getState().savedSearches.map((search) => search.id);
    expect(ids[0]).toBe(firstId);
    expect(ids[1]).toBe(secondId);
  });

  it('should duplicate a saved search with a unique name', () => {
    renderHook(() => useFileBrowserStore());

    let originalId = '';
    let duplicateId = '';
    act(() => {
      useFileBrowserStore.getState().setSearchQuery('glucosinolate');
      originalId = useFileBrowserStore.getState().createSavedSearch('Compound Search') ?? '';
      duplicateId = useFileBrowserStore.getState().duplicateSavedSearch(originalId) ?? '';
    });

    const searches = useFileBrowserStore.getState().savedSearches;
    const duplicate = searches.find((search) => search.id === duplicateId);
    expect(originalId).not.toBe('');
    expect(duplicateId).not.toBe('');
    expect(duplicate).toBeTruthy();
    expect(duplicate?.name).toBe('Compound Search Copy');
    expect(duplicate?.query).toBe('glucosinolate');
  });
});
