/**
 * FileBrowserStore Regression Tests
 *
 * Tests for P0-3: Infinite Loop in FileBrowserStore
 * Ensures Zustand store works correctly without infinite re-renders
 */

import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import type { DocumentMetadata } from '@/types/fileBrowser';

import {
  filterLibraryDocuments,
  sortLibraryDocuments,
  useFileBrowserStore,
} from './fileBrowserStore';


const sampleDocuments: DocumentMetadata[] = [
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
];

describe('FileBrowserStore - Zustand Implementation (P0-3 Regression)', () => {
  let renderCount = 0;

  beforeEach(() => {
    renderCount = 0;
    window.localStorage.clear();
    useFileBrowserStore.setState({
      viewMode: 'list',
      groupByDate: true,
      selectedDocumentIds: new Set(),
      sortField: 'name',
      sortOrder: 'asc',
      searchQuery: '',
      filterByType: null,
      filterBySource: 'all',
      contentSearchMatches: new Set(),
      isContentSearchLoading: false,
      savedSearches: [],
      activeSavedSearchId: null,
      sourceConnections: [],
      scope: { kind: 'all' },
      contextMenuPosition: null,
      contextMenuDocument: null,
    });
  });

  afterEach(() => {
  });

  it('should not cause infinite re-renders when changing sort field', async () => {
    const { result } = renderHook(() => {
      renderCount++;
      return useFileBrowserStore();
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

    const initialRenderCount = renderCount;

    // Change sort order
    act(() => {
      result.current.toggleSortOrder();
    });

    await new Promise((resolve) => setTimeout(resolve, 100));

    const finalRenderCount = renderCount;

    expect(finalRenderCount - initialRenderCount).toBeLessThan(10);
  });

  it('sorts query-owned documents without mutating the canonical array', () => {
    const byName = sortLibraryDocuments(sampleDocuments, 'name', 'asc');
    const bySize = sortLibraryDocuments(sampleDocuments, 'size', 'asc');

    expect(byName.map(document => document.fileName)).toEqual(['document.pdf', 'test.txt']);
    expect(bySize.map(document => document.fileName)).toEqual(['test.txt', 'document.pdf']);
    expect(sampleDocuments.map(document => document.fileName)).toEqual(['test.txt', 'document.pdf']);
  });

  it('selects only the document ids supplied by the query-derived view', () => {
    const { result } = renderHook(() => useFileBrowserStore());
    act(() => result.current.selectAll(['2']));
    expect([...result.current.selectedDocumentIds]).toEqual(['2']);
  });

  it('should keep backend-owned local folders out of client source state', () => {
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

    expect(useFileBrowserStore.getState().sourceConnections.some(connection => connection.id === cloudId)).toBe(true);
    expect(() => useFileBrowserStore.getState().createSourceConnection({
      name: 'Documents',
      provider: 'local_folder',
      mode: 'referenced',
      health: 'healthy',
      enabled: true,
      path: '/Users/me/Documents',
      lastSyncedAt: '2026-02-07T00:00:00.000Z',
    })).toThrow('Local folders are backend-owned');
    expect(useFileBrowserStore.getState().sourceConnections).toHaveLength(1);

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
      useFileBrowserStore.getState().setContentSearchMatches(['1']);
      useFileBrowserStore.getState().setSearchQuery('beta');
    });

    expect(useFileBrowserStore.getState().contentSearchMatches.size).toBe(0);
  });

  it('should scope results to a collection and its descendants', () => {
    renderHook(() => useFileBrowserStore());
    const documents: DocumentMetadata[] = [
      {
        id: '1', fileName: 'alpha.txt', filePath: '/alpha.txt', fileType: 'txt',
        category: 'document', language: 'en', modifiedAt: '2026-01-01',
        indexedAt: '2026-01-01', wordCount: 10,
      },
      {
        id: '2', fileName: 'beta.txt', filePath: '/beta.txt', fileType: 'txt',
        category: 'document', language: 'en', modifiedAt: '2026-01-01',
        indexedAt: '2026-01-01', wordCount: 10,
      },
    ];
    const state = useFileBrowserStore.getState();
    const collections = [
      { id: 'parent', name: 'Parent', kind: 'manual' as const, parentId: null, documentIds: [], createdAt: '', updatedAt: '' },
      { id: 'child', name: 'Child', kind: 'manual' as const, parentId: 'parent', documentIds: ['1'], createdAt: '', updatedAt: '' },
    ];
    const filtered = filterLibraryDocuments(documents, {
      searchQuery: state.searchQuery,
      filterByType: state.filterByType,
      filterBySource: state.filterBySource,
      contentSearchMatches: state.contentSearchMatches,
      customCollections: collections,
      scope: { kind: 'collection', id: 'parent' },
    });
    expect(filtered).toHaveLength(1);
    expect(filtered[0]?.id).toBe('1');
  });

  it('should scope results to a folder path', () => {
    renderHook(() => useFileBrowserStore());
    const documents: DocumentMetadata[] = [
      {
        id: '1', fileName: 'alpha.txt', filePath: '/Users/example/Notes/alpha.txt', fileType: 'txt',
        category: 'document', language: 'en', modifiedAt: '2026-01-01',
        indexedAt: '2026-01-01', wordCount: 10,
      },
      {
        id: '2', fileName: 'beta.txt', filePath: '/Users/example/Research/beta.txt', fileType: 'txt',
        category: 'document', language: 'en', modifiedAt: '2026-01-01',
        indexedAt: '2026-01-01', wordCount: 10,
      },
    ];

    act(() => {
      useFileBrowserStore.getState().setScope({ kind: 'folder', path: '/Users/example/Notes' });
    });

    const current = useFileBrowserStore.getState();
    const filtered = filterLibraryDocuments(documents, {
      searchQuery: current.searchQuery,
      filterByType: current.filterByType,
      filterBySource: current.filterBySource,
      contentSearchMatches: current.contentSearchMatches,
      customCollections: [],
      scope: current.scope,
    });
    expect(filtered).toHaveLength(1);
    expect(filtered[0]?.id).toBe('1');
  });

  it('should clear the selection when the scope changes', () => {
    renderHook(() => useFileBrowserStore());

    act(() => {
      useFileBrowserStore.getState().selectAll(['1', '2']);
      useFileBrowserStore.getState().setScope({ kind: 'folder', path: '/Users/example/Notes' });
    });

    expect(useFileBrowserStore.getState().selectedDocumentIds.size).toBe(0);
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


describe('focus vs selection', () => {
  it('clearSelection keeps the focused document', () => {
    const store = useFileBrowserStore.getState();
    store.setFocusedDocument('doc-focus');
    store.selectFile('doc-focus');
    store.clearSelection();
    expect(useFileBrowserStore.getState().selectedDocumentIds.size).toBe(0);
    expect(useFileBrowserStore.getState().focusedDocumentId).toBe('doc-focus');
  });
});
