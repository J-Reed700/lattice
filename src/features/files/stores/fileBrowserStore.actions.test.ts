import { beforeEach, describe, expect, it } from 'vitest';

import type { DocumentMetadata } from '@/types/fileBrowser';

import {
  selectActiveSavedSearchId,
  selectContentSearchLoading,
  selectContextMenuDocument,
  selectFilterBySource,
  selectFilterByType,
  selectGroupByDate,
  selectHasSelection,
  selectSavedSearches,
  selectScope,
  selectSearchQuery,
  selectSelectedCount,
  selectSelectedDocumentIds,
  selectSortField,
  selectSortOrder,
  selectViewMode,
  useFileBrowserStore,
} from './fileBrowserStore';

const store = () => useFileBrowserStore.getState();
const initial = useFileBrowserStore.getState();

describe('fileBrowserStore actions', () => {
  beforeEach(() => {
    useFileBrowserStore.setState(initial, true);
  });

  it('switches view and grouping', () => {
    store().setViewMode('grid');
    store().toggleGroupByDate();
    expect(selectViewMode(store())).toBe('grid');
    expect(selectGroupByDate(store())).toBe(!initial.groupByDate);
  });

  it('selects, deselects and toggles documents', () => {
    store().selectFile('d1');
    store().selectFile('d2');
    store().deselectFile('d1');
    store().toggleSelection('d3');
    store().toggleSelection('d2');
    expect([...selectSelectedDocumentIds(store())]).toEqual(['d3']);
    expect(selectSelectedCount(store())).toBe(1);
    expect(selectHasSelection(store())).toBe(true);
  });

  it('focuses a document and toggles its neighbourhood', () => {
    store().setFocusedDocument('d1');
    store().toggleNeighborhood();
    expect(store()).toMatchObject({ focusedDocumentId: 'd1', isNeighborhoodOpen: true });
  });

  it('sorts and flips the order', () => {
    store().setSortField('size');
    store().setSortOrder('asc');
    store().toggleSortOrder();
    expect(selectSortField(store())).toBe('size');
    expect(selectSortOrder(store())).toBe('desc');
    store().toggleSortOrder();
    expect(selectSortOrder(store())).toBe('asc');
  });

  it('keeps the state when the search does not change', () => {
    store().setSearchQuery('saffron');
    const before = store();
    store().setSearchQuery('saffron');
    expect(store()).toBe(before);
    expect(selectSearchQuery(store())).toBe('saffron');
  });

  it('normalises the type filter, treating all as none', () => {
    store().setFilterByType(' PDF ');
    expect(selectFilterByType(store())).toBe('pdf');
    store().setFilterByType('All');
    expect(selectFilterByType(store())).toBeNull();
    store().setFilterByType(null);
    expect(selectFilterByType(store())).toBeNull();
    store().setFilterBySource('web');
    expect(selectFilterBySource(store())).toBe('web');
    expect(selectActiveSavedSearchId(store())).toBeNull();
  });

  it('tracks content search state', () => {
    store().setContentSearchMatches(['d1', 'd2']);
    store().setContentSearchLoading(true);
    expect(selectContentSearchLoading(store())).toBe(true);
    expect(store().contentSearchMatches.size).toBe(2);
    store().clearContentSearch();
    expect(store().contentSearchMatches.size).toBe(0);
    expect(selectContentSearchLoading(store())).toBe(false);
  });

  it('opens and closes the context menu', () => {
    const doc = { id: 'd1' } as DocumentMetadata;
    store().openContextMenu({ x: 3, y: 4 }, doc);
    expect(selectContextMenuDocument(store())).toEqual({ position: { x: 3, y: 4 }, document: doc });
    store().closeContextMenu();
    expect(selectContextMenuDocument(store())).toEqual({ position: null, document: null });
    expect(selectScope(store())).toEqual({ kind: 'all' });
    expect(selectSavedSearches(store())).toEqual([]);
  });
});
