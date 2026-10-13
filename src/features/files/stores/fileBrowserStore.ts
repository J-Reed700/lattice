/**
 * File Browser Store - Zustand Implementation
 *
 * Manages file browser state with efficient subscriptions
 * Uses Zustand for performance and simplicity
 */

import { create } from 'zustand';

import {
  type LibraryScope,
  type SourceFilter,
  type ViewMode,
  type SortField,
  type SortOrder,
  type DocumentMetadata,
} from '@/types/fileBrowser';

import { loadSavedSearches, loadSourceConnections } from './fileBrowserPersistence';
import { type FileBrowserStore } from './fileBrowserStore.types';
import { normalizeToken } from './libraryDocumentFilters';
import { createSavedSearchActions } from './savedSearchActions';
import { createSourceConnectionActions } from './sourceConnectionActions';

export {
  filterLibraryDocuments,
  sortLibraryDocuments,
  type LibraryFilterState,
} from './libraryDocumentFilters';

export const useFileBrowserStore = create<FileBrowserStore>((set, get) => ({
  // Initial state
  viewMode: 'tree',
  groupByDate: false,
  sortField: 'name',
  sortOrder: 'asc',
  searchQuery: '',
  filterByType: null,
  filterBySource: 'all',
  contentSearchMatches: new Set(),
  isContentSearchLoading: false,
  savedSearches: loadSavedSearches(),
  activeSavedSearchId: null,
  sourceConnections: loadSourceConnections(),
  scope: { kind: 'all' },
  selectedDocumentIds: new Set(),
  focusedDocumentId: null,
  isNeighborhoodOpen: false,
  contextMenuPosition: null,
  contextMenuDocument: null,

  // View management
  setViewMode: (mode: ViewMode) => {
    set({ viewMode: mode });
  },

  toggleGroupByDate: () => {
    set((state) => ({ groupByDate: !state.groupByDate }));
  },

  setScope: (scope: LibraryScope) => {
    set({ scope, selectedDocumentIds: new Set() });
  },

  // Selection actions
  selectFile: (fileId: string) => {
    set((state) => ({
      selectedDocumentIds: new Set([...state.selectedDocumentIds, fileId]),
    }));
  },

  deselectFile: (fileId: string) => {
    set((state) => {
      const newSelection = new Set(state.selectedDocumentIds);
      newSelection.delete(fileId);
      return { selectedDocumentIds: newSelection };
    });
  },

  toggleSelection: (fileId: string) => {
    set((state) => {
      const newSelection = new Set(state.selectedDocumentIds);
      if (newSelection.has(fileId)) {
        newSelection.delete(fileId);
      } else {
        newSelection.add(fileId);
      }
      return { selectedDocumentIds: newSelection };
    });
  },

  selectAll: (fileIds: Iterable<string>) => {
    set({ selectedDocumentIds: new Set(fileIds) });
  },

  // Selection and focus are different things: focus is "the row you last
  // clicked" and is what the neighborhood panel is about. A plain click
  // clears the selection before selecting the clicked row, so clearing
  // focus here would blank the panel on every click.
  clearSelection: () => {
    set({ selectedDocumentIds: new Set() });
  },

  setFocusedDocument: (documentId: string | null) => {
    set({ focusedDocumentId: documentId });
  },

  toggleNeighborhood: () => {
    set((state) => ({ isNeighborhoodOpen: !state.isNeighborhoodOpen }));
  },

  // Sorting and filtering
  setSortField: (field: SortField) => {
    set({
      sortField: field,
    });
  },

  setSortOrder: (order: SortOrder) => {
    set({
      sortOrder: order,
    });
  },

  toggleSortOrder: () => {
    const { sortOrder } = get();
    const newOrder = sortOrder === 'asc' ? 'desc' : 'asc';
    set({
      sortOrder: newOrder,
    });
  },

  setSearchQuery: (query: string) => {
    set((state) => {
      if (state.searchQuery === query) {
        return state;
      }
      return {
        searchQuery: query,
        contentSearchMatches: new Set(),
        isContentSearchLoading: false,
        activeSavedSearchId: null,
      };
    });
  },

  setFilterByType: (type: string | null) => {
    const normalized = type ? normalizeToken(type) : null;
    set({
      filterByType: normalized && normalized !== 'all' ? normalized : null,
      activeSavedSearchId: null,
    });
  },

  setFilterBySource: (source: SourceFilter) => {
    set({ filterBySource: source, activeSavedSearchId: null });
  },

  setContentSearchMatches: (matches: Iterable<string>) => {
    set({ contentSearchMatches: new Set(matches) });
  },

  setContentSearchLoading: (isLoading: boolean) => {
    set({ isContentSearchLoading: isLoading });
  },

  clearContentSearch: () => {
    set({
      contentSearchMatches: new Set(),
      isContentSearchLoading: false,
    });
  },

  ...createSavedSearchActions(set, get),

  ...createSourceConnectionActions(set),

  // Context menu
  openContextMenu: (position: { x: number; y: number }, document: DocumentMetadata) => {
    set({
      contextMenuPosition: position,
      contextMenuDocument: document,
    });
  },

  closeContextMenu: () => {
    set({
      contextMenuPosition: null,
      contextMenuDocument: null,
    });
  },
}));

// Selectors for optimized component subscriptions
export const selectViewMode = (state: FileBrowserStore) => state.viewMode;
export const selectSelectedDocumentIds = (state: FileBrowserStore) => state.selectedDocumentIds;
export const selectSortField = (state: FileBrowserStore) => state.sortField;
export const selectSortOrder = (state: FileBrowserStore) => state.sortOrder;
export const selectSearchQuery = (state: FileBrowserStore) => state.searchQuery;
export const selectFilterByType = (state: FileBrowserStore) => state.filterByType;
export const selectFilterBySource = (state: FileBrowserStore) => state.filterBySource;
export const selectSavedSearches = (state: FileBrowserStore) => state.savedSearches;
export const selectActiveSavedSearchId = (state: FileBrowserStore) => state.activeSavedSearchId;
export const selectContextMenuDocument = (state: FileBrowserStore) => ({
  position: state.contextMenuPosition,
  document: state.contextMenuDocument,
});

// Computed selectors
export const selectSelectedCount = (state: FileBrowserStore) => state.selectedDocumentIds.size;
export const selectHasSelection = (state: FileBrowserStore) => state.selectedDocumentIds.size > 0;
export const selectGroupByDate = (state: FileBrowserStore) => state.groupByDate;
export const selectScope = (state: FileBrowserStore) => state.scope;
export const selectContentSearchLoading = (state: FileBrowserStore) => state.isContentSearchLoading;
