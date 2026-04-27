/**
 * File Browser Store - Zustand Implementation
 *
 * Manages file browser state with efficient subscriptions
 * Uses Zustand for performance and simplicity
 */

import { create } from 'zustand';

import VaultAPI from '../lib/api';
import {
  type CustomCollection,
  type FileNode,
  type FileBrowserActions,
  type ListColumnKey,
  type ListColumnVisibility,
  type SavedSearchPreset,
  type SourceConnection,
  type SourceFilter,
  type ViewMode,
  type SortField,
  type SortOrder,
  type DocumentMetadata,
  type SavedLibraryView,
} from '../types/fileBrowser';
import { getDateGroup, groupBy, type DateGroup } from '../utils/dateUtils';

export type Density = 'compact' | 'comfortable' | 'spacious';

export type ListItem =
  | { type: 'header'; label: DateGroup }
  | { type: 'doc'; data: DocumentMetadata };

interface FileBrowserState {
  // View configuration
  viewMode: ViewMode;
  density: Density;
  groupByDate: boolean;

  // Sorting and filtering
  sortField: SortField;
  sortOrder: SortOrder;
  searchQuery: string;
  filterByType: string | null;
  filterBySource: SourceFilter;
  contentSearchMatches: Set<string>;
  isContentSearchLoading: boolean;

  // Data
  documents: DocumentMetadata[];
  customCollections: CustomCollection[];
  savedViews: SavedLibraryView[];
  activeSavedViewId: string | null;
  savedSearches: SavedSearchPreset[];
  activeSavedSearchId: string | null;
  sourceConnections: SourceConnection[];
  isLoading: boolean;
  error: string | null;
  listColumns: ListColumnVisibility;

  // Selection
  selectedDocumentIds: Set<string>;

  // Context menu
  contextMenuPosition: { x: number; y: number } | null;
  contextMenuDocument: DocumentMetadata | null;
}

interface FileBrowserStore extends FileBrowserState, FileBrowserActions {}

const sortDocuments = (
  documents: DocumentMetadata[],
  sortField: SortField,
  sortOrder: SortOrder
): DocumentMetadata[] => {
  const direction = sortOrder === 'asc' ? 1 : -1;
  const toComparable = (doc: DocumentMetadata): string | number => {
    if (sortField === 'name') {
      return doc.fileName.toLowerCase();
    }
    if (sortField === 'size') {
      return doc.wordCount;
    }
    if (sortField === 'modified') {
      return new Date(doc.modifiedAt).getTime();
    }
    return doc.fileType.toLowerCase();
  };

  return documents
    .map((doc, index) => ({ doc, index, value: toComparable(doc) }))
    .sort((a, b) => {
      if (a.value < b.value) return -1 * direction;
      if (a.value > b.value) return 1 * direction;
      return a.index - b.index;
    })
    .map(({ doc }) => doc);
};

const normalizeToken = (value: string): string => value.trim().toLowerCase();
const isHttpUrl = (value: string): boolean => /^https?:\/\//i.test(value);
const isWebDocument = (doc: DocumentMetadata): boolean => {
  const normalizedCategory = normalizeToken(doc.category).replace(/[_-]+/g, ' ');
  return (
    normalizedCategory.includes('web article') ||
    normalizedCategory === 'web' ||
    isHttpUrl(doc.filePath) ||
    doc.filePath.includes('/.lattice/web-archive/')
  );
};
const CUSTOM_COLLECTIONS_STORAGE_KEY = 'lattice:file-browser-custom-collections:v1';
const SOURCE_CONNECTIONS_STORAGE_KEY = 'lattice:file-browser-source-connections:v1';
const SAVED_VIEWS_STORAGE_KEY = 'lattice:file-browser-saved-views:v1';
const SAVED_SEARCHES_STORAGE_KEY = 'lattice:file-browser-saved-searches:v1';
const DEFAULT_LIST_COLUMNS: ListColumnVisibility = {
  words: true,
  modified: true,
  type: true,
};

const sanitizeCollectionName = (value: string): string => value.trim().replace(/\s+/g, ' ');
const sanitizeSavedViewName = (value: string): string => value.trim().replace(/\s+/g, ' ');
const buildUniqueName = (candidate: string, existingNames: string[]): string => {
  const normalizedExisting = new Set(existingNames.map((name) => normalizeToken(name)));
  let nextName = sanitizeSavedViewName(candidate);
  if (!nextName) {
    nextName = 'Untitled';
  }
  if (!normalizedExisting.has(normalizeToken(nextName))) {
    return nextName;
  }

  let copyIndex = 2;
  while (normalizedExisting.has(normalizeToken(`${nextName} (${copyIndex})`))) {
    copyIndex += 1;
  }

  return `${nextName} (${copyIndex})`;
};
const isSameCollectionParent = (
  collection: CustomCollection,
  parentId: string | null
): boolean => (collection.parentId ?? null) === parentId;

const collectDescendantCollectionIds = (
  collections: CustomCollection[],
  collectionId: string
): Set<string> => {
  const descendants = new Set<string>();
  const queue = [collectionId];

  while (queue.length > 0) {
    const current = queue.shift();
    if (!current) {
      continue;
    }

    for (const collection of collections) {
      if (collection.parentId === current && !descendants.has(collection.id)) {
        descendants.add(collection.id);
        queue.push(collection.id);
      }
    }
  }

  return descendants;
};

const loadCustomCollections = (): CustomCollection[] => {
  if (typeof window === 'undefined') {
    return [];
  }

  try {
    const raw = window.localStorage.getItem(CUSTOM_COLLECTIONS_STORAGE_KEY);
    if (!raw) {
      return [];
    }

    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [];
    }

    const collections = parsed
      .filter((item) => item && typeof item === 'object')
      .map((item): CustomCollection | null => {
        const id = typeof item.id === 'string' ? item.id : '';
        const name = typeof item.name === 'string' ? sanitizeCollectionName(item.name) : '';
        const kind =
          item.kind === 'snapshot' || item.kind === 'manual'
            ? item.kind
            : 'manual';
        const parentId = typeof item.parentId === 'string' ? item.parentId : null;
        const documentIds = Array.isArray(item.documentIds)
          ? item.documentIds.filter((docId: unknown): docId is string => typeof docId === 'string')
          : [];
        const createdAt = typeof item.createdAt === 'string' ? item.createdAt : new Date().toISOString();
        const updatedAt = typeof item.updatedAt === 'string' ? item.updatedAt : createdAt;

        if (!id || !name) {
          return null;
        }

        return {
          id,
          name,
          kind,
          parentId,
          documentIds: Array.from(new Set(documentIds)),
          createdAt,
          updatedAt,
        };
      })
      .filter((item): item is CustomCollection => item !== null);

    const collectionIds = new Set(collections.map(collection => collection.id));
    return collections.map((collection) => ({
      ...collection,
      parentId:
        collection.parentId && collectionIds.has(collection.parentId)
          ? collection.parentId
          : null,
    }));
  } catch {
    return [];
  }
};

const persistCustomCollections = (collections: CustomCollection[]): void => {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(CUSTOM_COLLECTIONS_STORAGE_KEY, JSON.stringify(collections));
  } catch {
    // Ignore persistence failures; in-memory state remains valid
  }
};

const sanitizeSavedView = (value: unknown): SavedLibraryView | null => {
  if (!value || typeof value !== 'object') {
    return null;
  }

  const candidate = value as Partial<SavedLibraryView>;
  const viewMode = typeof candidate.viewMode === 'string' ? candidate.viewMode : '';
  const density = typeof candidate.density === 'string' ? candidate.density : '';
  const sortField = typeof candidate.sortField === 'string' ? candidate.sortField : '';
  const sortOrder = typeof candidate.sortOrder === 'string' ? candidate.sortOrder : '';
  const sourceFilter = typeof candidate.filterBySource === 'string' ? candidate.filterBySource : 'all';
  const groupByDate = Boolean(candidate.groupByDate);
  const rawListColumns = candidate.listColumns;
  const listColumns: ListColumnVisibility = {
    words:
      typeof rawListColumns === 'object' &&
      rawListColumns !== null &&
      'words' in rawListColumns &&
      typeof (rawListColumns as ListColumnVisibility).words === 'boolean'
        ? (rawListColumns as ListColumnVisibility).words
        : DEFAULT_LIST_COLUMNS.words,
    modified:
      typeof rawListColumns === 'object' &&
      rawListColumns !== null &&
      'modified' in rawListColumns &&
      typeof (rawListColumns as ListColumnVisibility).modified === 'boolean'
        ? (rawListColumns as ListColumnVisibility).modified
        : DEFAULT_LIST_COLUMNS.modified,
    type:
      typeof rawListColumns === 'object' &&
      rawListColumns !== null &&
      'type' in rawListColumns &&
      typeof (rawListColumns as ListColumnVisibility).type === 'boolean'
        ? (rawListColumns as ListColumnVisibility).type
        : DEFAULT_LIST_COLUMNS.type,
  };
  const baseCollectionId =
    typeof candidate.baseCollectionId === 'string' && candidate.baseCollectionId.trim()
      ? candidate.baseCollectionId
      : null;

  if (
    typeof candidate.id !== 'string' ||
    typeof candidate.name !== 'string' ||
    typeof candidate.searchQuery !== 'string' ||
    typeof candidate.createdAt !== 'string' ||
    typeof candidate.updatedAt !== 'string' ||
    !['tree', 'list', 'grid'].includes(viewMode) ||
    !['compact', 'comfortable', 'spacious'].includes(density) ||
    !['name', 'size', 'modified', 'type'].includes(sortField) ||
    !['asc', 'desc'].includes(sortOrder) ||
    !['all', 'local', 'web'].includes(sourceFilter)
  ) {
    return null;
  }

  return {
    id: candidate.id,
    name: sanitizeSavedViewName(candidate.name),
    baseCollectionId,
    viewMode: viewMode as ViewMode,
    density: density as Density,
    sortField: sortField as SortField,
    sortOrder: sortOrder as SortOrder,
    filterByType:
      typeof candidate.filterByType === 'string' && normalizeToken(candidate.filterByType)
        ? normalizeToken(candidate.filterByType)
        : null,
    filterBySource: sourceFilter as SourceFilter,
    groupByDate,
    searchQuery: candidate.searchQuery,
    listColumns,
    createdAt: candidate.createdAt,
    updatedAt: candidate.updatedAt,
  };
};

const loadSavedViews = (): SavedLibraryView[] => {
  if (typeof window === 'undefined') {
    return [];
  }

  try {
    const raw = window.localStorage.getItem(SAVED_VIEWS_STORAGE_KEY);
    if (!raw) {
      return [];
    }

    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [];
    }

    return parsed
      .map((item) => sanitizeSavedView(item))
      .filter((item): item is SavedLibraryView => item !== null);
  } catch {
    return [];
  }
};

const persistSavedViews = (views: SavedLibraryView[]): void => {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(SAVED_VIEWS_STORAGE_KEY, JSON.stringify(views));
  } catch {
    // Ignore persistence failures; in-memory state remains valid
  }
};

const sanitizeSavedSearch = (value: unknown): SavedSearchPreset | null => {
  if (!value || typeof value !== 'object') {
    return null;
  }

  const candidate = value as Partial<SavedSearchPreset>;
  const sourceFilter = typeof candidate.filterBySource === 'string' ? candidate.filterBySource : 'all';
  if (
    typeof candidate.id !== 'string' ||
    typeof candidate.name !== 'string' ||
    typeof candidate.query !== 'string' ||
    typeof candidate.createdAt !== 'string' ||
    typeof candidate.updatedAt !== 'string' ||
    !['all', 'local', 'web'].includes(sourceFilter)
  ) {
    return null;
  }

  return {
    id: candidate.id,
    name: sanitizeSavedViewName(candidate.name),
    query: candidate.query,
    filterByType:
      typeof candidate.filterByType === 'string' && normalizeToken(candidate.filterByType)
        ? normalizeToken(candidate.filterByType)
        : null,
    filterBySource: sourceFilter as SourceFilter,
    pinned: Boolean(candidate.pinned),
    createdAt: candidate.createdAt,
    updatedAt: candidate.updatedAt,
  };
};

const loadSavedSearches = (): SavedSearchPreset[] => {
  if (typeof window === 'undefined') {
    return [];
  }

  try {
    const raw = window.localStorage.getItem(SAVED_SEARCHES_STORAGE_KEY);
    if (!raw) {
      return [];
    }

    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [];
    }

    return parsed
      .map((item) => sanitizeSavedSearch(item))
      .filter((item): item is SavedSearchPreset => item !== null);
  } catch {
    return [];
  }
};

const persistSavedSearches = (searches: SavedSearchPreset[]): void => {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(SAVED_SEARCHES_STORAGE_KEY, JSON.stringify(searches));
  } catch {
    // Ignore persistence failures; in-memory state remains valid
  }
};

const sanitizeSourceConnection = (item: unknown): SourceConnection | null => {
  if (!item || typeof item !== 'object') {
    return null;
  }

  const candidate = item as Partial<SourceConnection>;
  if (
    typeof candidate.id !== 'string' ||
    typeof candidate.name !== 'string' ||
    typeof candidate.provider !== 'string' ||
    typeof candidate.mode !== 'string' ||
    typeof candidate.health !== 'string' ||
    typeof candidate.enabled !== 'boolean' ||
    typeof candidate.createdAt !== 'string' ||
    typeof candidate.updatedAt !== 'string'
  ) {
    return null;
  }

  if (
    !['local_folder', 'web_import', 'dropbox', 'google_drive', 'onedrive', 'icloud'].includes(candidate.provider) ||
    !['referenced', 'managed'].includes(candidate.mode) ||
    !['healthy', 'attention', 'error', 'paused'].includes(candidate.health)
  ) {
    return null;
  }

  return {
    id: candidate.id,
    name: candidate.name,
    provider: candidate.provider,
    mode: candidate.mode,
    health: candidate.health,
    enabled: candidate.enabled,
    path: typeof candidate.path === 'string' ? candidate.path : undefined,
    lastSyncedAt: typeof candidate.lastSyncedAt === 'string' ? candidate.lastSyncedAt : null,
    createdAt: candidate.createdAt,
    updatedAt: candidate.updatedAt,
  };
};

const loadSourceConnections = (): SourceConnection[] => {
  if (typeof window === 'undefined') {
    return [];
  }

  try {
    const raw = window.localStorage.getItem(SOURCE_CONNECTIONS_STORAGE_KEY);
    if (!raw) {
      return [];
    }

    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [];
    }

    return parsed
      .map((item) => sanitizeSourceConnection(item))
      .filter((item): item is SourceConnection => item !== null);
  } catch {
    return [];
  }
};

const persistSourceConnections = (connections: SourceConnection[]): void => {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(SOURCE_CONNECTIONS_STORAGE_KEY, JSON.stringify(connections));
  } catch {
    // Ignore persistence failures; in-memory state remains valid
  }
};

const sortSourceConnections = (connections: SourceConnection[]): SourceConnection[] =>
  [...connections].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));

export const useFileBrowserStore = create<FileBrowserStore>((set, get) => ({
  // Initial state
  viewMode: 'tree',
  density: 'comfortable',
  groupByDate: false,
  sortField: 'name',
  sortOrder: 'asc',
  searchQuery: '',
  filterByType: null,
  filterBySource: 'all',
  contentSearchMatches: new Set(),
  isContentSearchLoading: false,
  documents: [],
  customCollections: loadCustomCollections(),
  savedViews: loadSavedViews(),
  activeSavedViewId: null,
  savedSearches: loadSavedSearches(),
  activeSavedSearchId: null,
  sourceConnections: loadSourceConnections(),
  isLoading: false,
  error: null,
  listColumns: { ...DEFAULT_LIST_COLUMNS },
  selectedDocumentIds: new Set(),
  contextMenuPosition: null,
  contextMenuDocument: null,

  // View management
  setViewMode: (mode: ViewMode) => {
    set({ viewMode: mode, activeSavedViewId: null });
  },

  setDensity: (density: Density) => {
    set({ density, activeSavedViewId: null });
  },

  toggleGroupByDate: () => {
    set((state) => ({ groupByDate: !state.groupByDate, activeSavedViewId: null }));
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

  selectAll: () => {
    const { documents } = get();
    set({ selectedDocumentIds: new Set(documents.map((d) => d.id)) });
  },

  clearSelection: () => {
    set({ selectedDocumentIds: new Set() });
  },

  // Sorting and filtering
  setSortField: (field: SortField) => {
    set((state) => ({
      sortField: field,
      documents: sortDocuments(state.documents, field, state.sortOrder),
      activeSavedViewId: null,
    }));
  },

  setSortOrder: (order: SortOrder) => {
    set((state) => ({
      sortOrder: order,
      documents: sortDocuments(state.documents, state.sortField, order),
      activeSavedViewId: null,
    }));
  },

  toggleSortOrder: () => {
    const { sortOrder, sortField, documents } = get();
    const newOrder = sortOrder === 'asc' ? 'desc' : 'asc';
    set({
      sortOrder: newOrder,
      documents: sortDocuments(documents, sortField, newOrder),
      activeSavedViewId: null,
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
        activeSavedViewId: null,
        activeSavedSearchId: null,
      };
    });
  },

  setFilterByType: (type: string | null) => {
    const normalized = type ? normalizeToken(type) : null;
    set({
      filterByType: normalized && normalized !== 'all' ? normalized : null,
      activeSavedViewId: null,
      activeSavedSearchId: null,
    });
  },

  setFilterBySource: (source: SourceFilter) => {
    set({ filterBySource: source, activeSavedViewId: null, activeSavedSearchId: null });
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

  setListColumnVisibility: (updates: Partial<ListColumnVisibility>) => {
    set((state) => ({
      listColumns: {
        ...state.listColumns,
        ...updates,
      },
      activeSavedViewId: null,
    }));
  },

  toggleListColumnVisibility: (column: ListColumnKey) => {
    set((state) => ({
      listColumns: {
        ...state.listColumns,
        [column]: !state.listColumns[column],
      },
      activeSavedViewId: null,
    }));
  },

  createSavedView: (name: string) => {
    const normalizedName = sanitizeSavedViewName(name);
    if (!normalizedName) {
      return null;
    }

    const now = new Date().toISOString();
    const viewId = `view:${Date.now().toString(36)}:${Math.random().toString(36).slice(2, 9)}`;
    const state = get();
    const hasDuplicate = state.savedViews.some(
      (view) => normalizeToken(view.name) === normalizeToken(normalizedName)
    );
    if (hasDuplicate) {
      return null;
    }

    const nextView: SavedLibraryView = {
      id: viewId,
      name: normalizedName,
      baseCollectionId: null,
      viewMode: state.viewMode,
      density: state.density,
      sortField: state.sortField,
      sortOrder: state.sortOrder,
      filterByType: state.filterByType,
      filterBySource: state.filterBySource,
      groupByDate: state.groupByDate,
      searchQuery: state.searchQuery,
      listColumns: { ...state.listColumns },
      createdAt: now,
      updatedAt: now,
    };

    set((currentState) => {
      const savedViews = [nextView, ...currentState.savedViews];
      persistSavedViews(savedViews);
      return {
        savedViews,
        activeSavedViewId: viewId,
      };
    });

    return viewId;
  },

  applySavedView: (viewId: string) => {
    set((state) => {
      const target = state.savedViews.find((view) => view.id === viewId);
      if (!target) {
        return state;
      }

      return {
        activeSavedViewId: target.id,
        activeSavedSearchId: null,
        viewMode: target.viewMode,
        density: target.density,
        sortField: target.sortField,
        sortOrder: target.sortOrder,
        filterByType: target.filterByType,
        filterBySource: target.filterBySource,
        groupByDate: target.groupByDate,
        searchQuery: target.searchQuery,
        listColumns: { ...target.listColumns },
        contentSearchMatches: new Set(),
        isContentSearchLoading: false,
      };
    });
  },

  updateSavedView: (viewId, updates) => {
    set((state) => {
      const target = state.savedViews.find((view) => view.id === viewId);
      if (!target) {
        return state;
      }

      const normalizedName =
        typeof updates.name === 'string' ? sanitizeSavedViewName(updates.name) : target.name;
      if (!normalizedName) {
        return state;
      }

      const hasDuplicateName = state.savedViews.some(
        (view) =>
          view.id !== viewId &&
          normalizeToken(view.name) === normalizeToken(normalizedName)
      );
      if (hasDuplicateName) {
        return state;
      }

      const nextView: SavedLibraryView = {
        ...target,
        ...updates,
        name: normalizedName,
        baseCollectionId:
          typeof updates.baseCollectionId === 'string' && updates.baseCollectionId.trim()
            ? updates.baseCollectionId
            : updates.baseCollectionId === null
              ? null
              : target.baseCollectionId,
        filterByType:
          typeof updates.filterByType === 'string'
            ? normalizeToken(updates.filterByType) || null
            : updates.filterByType === null
              ? null
              : target.filterByType,
        listColumns:
          updates.listColumns
            ? {
                ...target.listColumns,
                ...updates.listColumns,
              }
            : target.listColumns,
        updatedAt: new Date().toISOString(),
      };

      const savedViews = state.savedViews.map((view) => (view.id === viewId ? nextView : view));
      persistSavedViews(savedViews);

      if (state.activeSavedViewId !== viewId) {
        return { savedViews };
      }

      return {
        savedViews,
        viewMode: nextView.viewMode,
        density: nextView.density,
        sortField: nextView.sortField,
        sortOrder: nextView.sortOrder,
        filterByType: nextView.filterByType,
        filterBySource: nextView.filterBySource,
        groupByDate: nextView.groupByDate,
        searchQuery: nextView.searchQuery,
        listColumns: { ...nextView.listColumns },
        contentSearchMatches: new Set(),
        isContentSearchLoading: false,
      };
    });
  },

  captureSavedViewState: (viewId) => {
    set((state) => {
      const target = state.savedViews.find((view) => view.id === viewId);
      if (!target) {
        return state;
      }

      const nextView: SavedLibraryView = {
        ...target,
        viewMode: state.viewMode,
        density: state.density,
        sortField: state.sortField,
        sortOrder: state.sortOrder,
        filterByType: state.filterByType,
        filterBySource: state.filterBySource,
        groupByDate: state.groupByDate,
        searchQuery: state.searchQuery,
        listColumns: { ...state.listColumns },
        updatedAt: new Date().toISOString(),
      };

      const savedViews = state.savedViews.map((view) => (view.id === viewId ? nextView : view));
      persistSavedViews(savedViews);
      return { savedViews };
    });
  },

  deleteSavedView: (viewId: string) => {
    set((state) => {
      const savedViews = state.savedViews.filter((view) => view.id !== viewId);
      persistSavedViews(savedViews);
      return {
        savedViews,
        activeSavedViewId: state.activeSavedViewId === viewId ? null : state.activeSavedViewId,
      };
    });
  },

  clearActiveSavedView: () => {
    set({ activeSavedViewId: null });
  },

  createSavedSearch: (name: string, options) => {
    const normalizedName = sanitizeSavedViewName(name);
    if (!normalizedName) {
      return null;
    }

    const state = get();
    const hasCriteria =
      Boolean(normalizeToken(state.searchQuery)) ||
      state.filterByType !== null ||
      state.filterBySource !== 'all';
    if (!hasCriteria) {
      return null;
    }

    const hasDuplicate = state.savedSearches.some(
      (search) => normalizeToken(search.name) === normalizeToken(normalizedName)
    );
    if (hasDuplicate) {
      return null;
    }

    const now = new Date().toISOString();
    const searchId = `search:${Date.now().toString(36)}:${Math.random().toString(36).slice(2, 9)}`;
    const nextSearch: SavedSearchPreset = {
      id: searchId,
      name: normalizedName,
      query: state.searchQuery,
      filterByType: state.filterByType,
      filterBySource: state.filterBySource,
      pinned: options?.pinned ?? true,
      createdAt: now,
      updatedAt: now,
    };

    set((currentState) => {
      const savedSearches = [nextSearch, ...currentState.savedSearches];
      persistSavedSearches(savedSearches);
      return {
        savedSearches,
        activeSavedSearchId: searchId,
      };
    });

    return searchId;
  },

  applySavedSearch: (searchId: string) => {
    set((state) => {
      const target = state.savedSearches.find((search) => search.id === searchId);
      if (!target) {
        return state;
      }

      return {
        activeSavedSearchId: target.id,
        activeSavedViewId: null,
        searchQuery: target.query,
        filterByType: target.filterByType,
        filterBySource: target.filterBySource,
        contentSearchMatches: new Set(),
        isContentSearchLoading: false,
      };
    });
  },

  updateSavedSearch: (searchId, updates) => {
    set((state) => {
      const target = state.savedSearches.find((search) => search.id === searchId);
      if (!target) {
        return state;
      }

      const normalizedName =
        typeof updates.name === 'string' ? sanitizeSavedViewName(updates.name) : target.name;
      if (!normalizedName) {
        return state;
      }

      const hasDuplicateName = state.savedSearches.some(
        (search) => search.id !== searchId && normalizeToken(search.name) === normalizeToken(normalizedName)
      );
      if (hasDuplicateName) {
        return state;
      }

      const nextSearch: SavedSearchPreset = {
        ...target,
        ...updates,
        name: normalizedName,
        updatedAt: new Date().toISOString(),
      };

      const savedSearches = state.savedSearches.map((search) => (search.id === searchId ? nextSearch : search));
      persistSavedSearches(savedSearches);
      return { savedSearches };
    });
  },

  reorderSavedSearches: (orderedIds: string[]) => {
    set((state) => {
      if (orderedIds.length === 0 || state.savedSearches.length < 2) {
        return state;
      }

      const desiredOrder = new Map<string, number>(
        orderedIds.map((id, index) => [id, index])
      );

      const ordered = [...state.savedSearches].sort((a, b) => {
        const left = desiredOrder.get(a.id);
        const right = desiredOrder.get(b.id);
        if (left === undefined && right === undefined) {
          return 0;
        }
        if (left === undefined) {
          return 1;
        }
        if (right === undefined) {
          return -1;
        }
        return left - right;
      });

      persistSavedSearches(ordered);
      return { savedSearches: ordered };
    });
  },

  duplicateSavedSearch: (searchId: string) => {
    const state = get();
    const source = state.savedSearches.find((search) => search.id === searchId);
    if (!source) {
      return null;
    }

    const now = new Date().toISOString();
    const duplicateId = `search:${Date.now().toString(36)}:${Math.random().toString(36).slice(2, 9)}`;
    const duplicateName = buildUniqueName(
      `${source.name} Copy`,
      state.savedSearches.map((search) => search.name)
    );

    const duplicate: SavedSearchPreset = {
      ...source,
      id: duplicateId,
      name: duplicateName,
      createdAt: now,
      updatedAt: now,
    };

    set((currentState) => {
      const sourceIndex = currentState.savedSearches.findIndex((search) => search.id === searchId);
      const insertIndex = sourceIndex >= 0 ? sourceIndex + 1 : currentState.savedSearches.length;
      const savedSearches = [...currentState.savedSearches];
      savedSearches.splice(insertIndex, 0, duplicate);
      persistSavedSearches(savedSearches);
      return { savedSearches };
    });

    return duplicateId;
  },

  deleteSavedSearch: (searchId: string) => {
    set((state) => {
      const savedSearches = state.savedSearches.filter((search) => search.id !== searchId);
      persistSavedSearches(savedSearches);
      return {
        savedSearches,
        activeSavedSearchId: state.activeSavedSearchId === searchId ? null : state.activeSavedSearchId,
      };
    });
  },

  clearActiveSavedSearch: () => {
    set({ activeSavedSearchId: null });
  },

  createCustomCollection: (name: string, parentCollectionId: string | null = null) => {
    const normalizedName = sanitizeCollectionName(name);
    if (!normalizedName) {
      return null;
    }

    const normalizedParentId = parentCollectionId?.trim() || null;
    const now = new Date().toISOString();
    const collectionId = `custom:${Date.now().toString(36)}:${Math.random().toString(36).slice(2, 9)}`;
    set((state) => {
      const parentExists =
        normalizedParentId === null ||
        state.customCollections.some(collection => collection.id === normalizedParentId);
      if (!parentExists) {
        return state;
      }

      const hasDuplicate = state.customCollections.some(
        collection =>
          isSameCollectionParent(collection, normalizedParentId) &&
          normalizeToken(collection.name) === normalizeToken(normalizedName)
      );

      if (hasDuplicate) {
        return state;
      }

      const newCollection: CustomCollection = {
        id: collectionId,
        name: normalizedName,
        kind: 'manual',
        parentId: normalizedParentId,
        documentIds: [],
        createdAt: now,
        updatedAt: now,
      };

      const customCollections = [newCollection, ...state.customCollections];
      persistCustomCollections(customCollections);
      return { customCollections };
    });

    const created = get().customCollections.some(collection => collection.id === collectionId);
    return created ? collectionId : null;
  },

  createSnapshotCollection: (name: string, documentIds: string[], parentCollectionId: string | null = null) => {
    const normalizedName = sanitizeCollectionName(name);
    if (!normalizedName) {
      return null;
    }

    const snapshotIds = Array.from(new Set(documentIds.filter((id) => typeof id === 'string' && id)));
    if (snapshotIds.length === 0) {
      return null;
    }

    const normalizedParentId = parentCollectionId?.trim() || null;
    const now = new Date().toISOString();
    const collectionId = `custom:${Date.now().toString(36)}:${Math.random().toString(36).slice(2, 9)}`;

    set((state) => {
      const parentExists =
        normalizedParentId === null ||
        state.customCollections.some(collection => collection.id === normalizedParentId);
      if (!parentExists) {
        return state;
      }

      const hasDuplicate = state.customCollections.some(
        (collection) =>
          isSameCollectionParent(collection, normalizedParentId) &&
          normalizeToken(collection.name) === normalizeToken(normalizedName)
      );

      if (hasDuplicate) {
        return state;
      }

      const newCollection: CustomCollection = {
        id: collectionId,
        name: normalizedName,
        kind: 'snapshot',
        parentId: normalizedParentId,
        documentIds: snapshotIds,
        createdAt: now,
        updatedAt: now,
      };

      const customCollections = [newCollection, ...state.customCollections];
      persistCustomCollections(customCollections);
      return { customCollections };
    });

    const created = get().customCollections.some((collection) => collection.id === collectionId);
    return created ? collectionId : null;
  },

  renameCustomCollection: (collectionId: string, name: string) => {
    const normalizedName = sanitizeCollectionName(name);
    if (!normalizedName) {
      return;
    }

    set((state) => {
      const targetCollection = state.customCollections.find(collection => collection.id === collectionId);
      if (!targetCollection) {
        return state;
      }

      const hasDuplicate = state.customCollections.some(
        collection =>
          collection.id !== collectionId &&
          isSameCollectionParent(collection, targetCollection.parentId ?? null) &&
          normalizeToken(collection.name) === normalizeToken(normalizedName)
      );

      if (hasDuplicate) {
        return state;
      }

      const customCollections = state.customCollections.map((collection) =>
        collection.id === collectionId
          ? { ...collection, name: normalizedName, updatedAt: new Date().toISOString() }
          : collection
      );
      persistCustomCollections(customCollections);
      return { customCollections };
    });
  },

  updateCustomCollection: (collectionId: string, updates) => {
    const hasNameUpdate = typeof updates.name === 'string';
    const hasParentUpdate = updates.parentId !== undefined;
    if (!hasNameUpdate && !hasParentUpdate) {
      return false;
    }

    let didUpdate = false;
    set((state) => {
      const targetCollection = state.customCollections.find((collection) => collection.id === collectionId);
      if (!targetCollection) {
        return state;
      }

      const nextName = hasNameUpdate
        ? sanitizeCollectionName(updates.name ?? '')
        : targetCollection.name;
      if (!nextName) {
        return state;
      }

      const nextParentId = hasParentUpdate
        ? (updates.parentId?.trim() || null)
        : (targetCollection.parentId ?? null);

      if (nextParentId === collectionId) {
        return state;
      }

      if (nextParentId !== null) {
        const parentExists = state.customCollections.some((collection) => collection.id === nextParentId);
        if (!parentExists) {
          return state;
        }

        const descendantIds = collectDescendantCollectionIds(state.customCollections, collectionId);
        if (descendantIds.has(nextParentId)) {
          return state;
        }
      }

      const hasDuplicate = state.customCollections.some(
        (collection) =>
          collection.id !== collectionId &&
          isSameCollectionParent(collection, nextParentId) &&
          normalizeToken(collection.name) === normalizeToken(nextName)
      );
      if (hasDuplicate) {
        return state;
      }

      const hasChanged =
        normalizeToken(targetCollection.name) !== normalizeToken(nextName) ||
        (targetCollection.parentId ?? null) !== nextParentId;
      if (!hasChanged) {
        return state;
      }

      const customCollections = state.customCollections.map((collection) =>
        collection.id === collectionId
          ? {
              ...collection,
              name: nextName,
              parentId: nextParentId,
              updatedAt: new Date().toISOString(),
            }
          : collection
      );
      didUpdate = true;
      persistCustomCollections(customCollections);
      return { customCollections };
    });

    return didUpdate;
  },

  deleteCustomCollection: (collectionId: string) => {
    set((state) => {
      const toDelete = new Set([collectionId]);
      let changed = true;

      while (changed) {
        changed = false;
        for (const collection of state.customCollections) {
          if (collection.parentId && toDelete.has(collection.parentId) && !toDelete.has(collection.id)) {
            toDelete.add(collection.id);
            changed = true;
          }
        }
      }

      const customCollections = state.customCollections.filter(collection => !toDelete.has(collection.id));
      persistCustomCollections(customCollections);
      return { customCollections };
    });
  },

  addDocumentsToCustomCollection: (collectionId: string, documentIds: string[]) => {
    if (documentIds.length === 0) {
      return;
    }

    set((state) => {
      const uniqueIds = Array.from(new Set(documentIds));
      const customCollections = state.customCollections.map((collection) => {
        if (collection.id !== collectionId) {
          return collection;
        }
        if (collection.kind === 'snapshot') {
          return collection;
        }

        const mergedIds = Array.from(new Set([...collection.documentIds, ...uniqueIds]));
        return {
          ...collection,
          documentIds: mergedIds,
          updatedAt: new Date().toISOString(),
        };
      });

      persistCustomCollections(customCollections);
      return { customCollections };
    });
  },

  removeDocumentsFromCustomCollection: (collectionId: string, documentIds: string[]) => {
    if (documentIds.length === 0) {
      return;
    }

    const removeSet = new Set(documentIds);

    set((state) => {
      const customCollections = state.customCollections.map((collection) => {
        if (collection.id !== collectionId) {
          return collection;
        }
        if (collection.kind === 'snapshot') {
          return collection;
        }

        return {
          ...collection,
          documentIds: collection.documentIds.filter(docId => !removeSet.has(docId)),
          updatedAt: new Date().toISOString(),
        };
      });

      persistCustomCollections(customCollections);
      return { customCollections };
    });
  },

  createSourceConnection: (connection) => {
    const now = new Date().toISOString();
    const sourceId = `source:${connection.provider}:${Date.now().toString(36)}:${Math.random().toString(36).slice(2, 9)}`;

    const newConnection: SourceConnection = {
      ...connection,
      id: sourceId,
      createdAt: now,
      updatedAt: now,
    };

    set((state) => {
      const sourceConnections = sortSourceConnections([newConnection, ...state.sourceConnections]);
      persistSourceConnections(sourceConnections);
      return { sourceConnections };
    });

    return sourceId;
  },

  reconcileLocalSources: (connections: SourceConnection[]) => {
    set((state) => {
      const nonLocalSources = state.sourceConnections.filter(connection => connection.provider !== 'local_folder');

      const mergedLocalSources = connections.map((incoming) => {
        const existing = state.sourceConnections.find(connection => connection.id === incoming.id);
        if (!existing) {
          return incoming;
        }

        return {
          ...incoming,
          mode: existing.mode,
          createdAt: existing.createdAt,
          updatedAt: new Date().toISOString(),
        };
      });

      const sourceConnections = sortSourceConnections([...nonLocalSources, ...mergedLocalSources]);
      persistSourceConnections(sourceConnections);
      return { sourceConnections };
    });
  },

  upsertSourceConnections: (connections: SourceConnection[]) => {
    if (connections.length === 0) {
      return;
    }

    set((state) => {
      const byId = new Map(state.sourceConnections.map(connection => [connection.id, connection]));
      for (const connection of connections) {
        byId.set(connection.id, connection);
      }

      const sourceConnections = sortSourceConnections(Array.from(byId.values()));
      persistSourceConnections(sourceConnections);
      return { sourceConnections };
    });
  },

  updateSourceConnection: (sourceId: string, updates: Partial<SourceConnection>) => {
    set((state) => {
      const sourceConnections = state.sourceConnections.map((connection) =>
        connection.id === sourceId
          ? {
              ...connection,
              ...updates,
              id: connection.id,
              createdAt: connection.createdAt,
              updatedAt: new Date().toISOString(),
            }
          : connection
      );

      persistSourceConnections(sourceConnections);
      return { sourceConnections };
    });
  },

  deleteSourceConnection: (sourceId: string) => {
    set((state) => {
      const sourceConnections = state.sourceConnections.filter(connection => connection.id !== sourceId);
      persistSourceConnections(sourceConnections);
      return { sourceConnections };
    });
  },

  // Data operations
  loadFiles: async () => {
    console.log('[FileBrowserStore] loadFiles called');
    set({ isLoading: true, error: null });

    try {
      // Query documents table to show ALL indexed documents
      console.log('[FileBrowserStore] Calling VaultAPI.listAllDocuments...');
      const result = await VaultAPI.listAllDocuments(10000);
      console.log('[FileBrowserStore] API result:', result);

      if (!result.ok) {
        console.error('[FileBrowserStore] API error:', result.error);
        set({
          error: result.error,
          isLoading: false,
        });
        return;
      }

      // Backend returns DocumentMetadata directly (camelCase from serde)
      const documents: DocumentMetadata[] = result.data;
      const { sortField, sortOrder } = get();
      const sortedDocuments = sortDocuments(documents, sortField, sortOrder);
      console.log('[FileBrowserStore] Documents received:', documents.length, documents);

      set({
        documents: sortedDocuments,
        isLoading: false,
        error: null,
        selectedDocumentIds: new Set(),
        contentSearchMatches: new Set(),
        isContentSearchLoading: false,
      });

      console.log('[FileBrowserStore] State updated. Documents length:', get().documents.length);
    } catch (err) {
      set({
        error: err instanceof Error ? err.message : 'Failed to load files',
        isLoading: false,
      });
    }
  },

  refreshFiles: async () => {
    await get().loadFiles();
  },

  // Context menu
  openContextMenu: (position: { x: number; y: number }, file: FileNode) => {
    // Find the corresponding document metadata by id
    const { documents } = get();
    const document = documents.find(d => d.id === file.id);

    set({
      contextMenuPosition: position,
      contextMenuDocument: document || null,
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
export const selectSavedViews = (state: FileBrowserStore) => state.savedViews;
export const selectActiveSavedViewId = (state: FileBrowserStore) => state.activeSavedViewId;
export const selectSavedSearches = (state: FileBrowserStore) => state.savedSearches;
export const selectActiveSavedSearchId = (state: FileBrowserStore) => state.activeSavedSearchId;
export const selectDocuments = (state: FileBrowserStore) => state.documents;
export const selectIsLoading = (state: FileBrowserStore) => state.isLoading;
export const selectError = (state: FileBrowserStore) => state.error;
export const selectListColumns = (state: FileBrowserStore) => state.listColumns;
export const selectContextMenuDocument = (state: FileBrowserStore) => ({
  position: state.contextMenuPosition,
  document: state.contextMenuDocument,
});

// Computed selectors
export const selectSelectedCount = (state: FileBrowserStore) => state.selectedDocumentIds.size;
export const selectHasSelection = (state: FileBrowserStore) => state.selectedDocumentIds.size > 0;
const buildFilteredDocumentsSelector = () => {
  let lastDocuments: DocumentMetadata[] | null = null;
  let lastCustomCollections: CustomCollection[] | null = null;
  let lastSavedViews: SavedLibraryView[] | null = null;
  let lastActiveSavedViewId: string | null = null;
  let lastQuery = '';
  let lastFilter: string | null = null;
  let lastSourceFilter: SourceFilter = 'all';
  let lastContentMatches: Set<string> | null = null;
  let lastResult: DocumentMetadata[] = [];

  return (state: FileBrowserStore): DocumentMetadata[] => {
    const query = normalizeToken(state.searchQuery);
    const normalizedFilter = state.filterByType ? normalizeToken(state.filterByType) : null;
    const sourceFilter = state.filterBySource;
    const activeSavedView = state.activeSavedViewId
      ? state.savedViews.find((view) => view.id === state.activeSavedViewId) ?? null
      : null;
    const baseCollectionId = activeSavedView?.baseCollectionId ?? null;
    const baseCollection =
      baseCollectionId
        ? state.customCollections.find((collection) => collection.id === baseCollectionId) ?? null
        : null;
    const baseCollectionDocIds = baseCollection ? new Set(baseCollection.documentIds) : null;

    if (
      state.documents === lastDocuments &&
      state.customCollections === lastCustomCollections &&
      state.savedViews === lastSavedViews &&
      state.activeSavedViewId === lastActiveSavedViewId &&
      query === lastQuery &&
      normalizedFilter === lastFilter &&
      sourceFilter === lastSourceFilter &&
      state.contentSearchMatches === lastContentMatches
    ) {
      return lastResult;
    }

    const { documents } = state;
    if (!query && !normalizedFilter && sourceFilter === 'all' && !baseCollectionDocIds) {
      lastDocuments = documents;
      lastCustomCollections = state.customCollections;
      lastSavedViews = state.savedViews;
      lastActiveSavedViewId = state.activeSavedViewId;
      lastQuery = query;
      lastFilter = normalizedFilter;
      lastSourceFilter = sourceFilter;
      lastContentMatches = state.contentSearchMatches;
      lastResult = documents;
      return lastResult;
    }

    const filtered = documents.filter((doc) => {
      const normalizedCategory = normalizeToken(doc.category).replace(/[_-]+/g, ' ');
      const normalizedFileType = normalizeToken(doc.fileType);
      const normalizedFileName = normalizeToken(doc.fileName);

      const matchesQuery = query
        ? normalizedFileName.includes(query) || state.contentSearchMatches.has(doc.id)
        : true;
      const matchesType = normalizedFilter
        ? normalizedCategory.includes(normalizedFilter) ||
          normalizedFileType === normalizedFilter ||
          (normalizedFilter === 'web' && normalizedCategory.includes('web article'))
        : true;
      const matchesSource =
        sourceFilter === 'all' ||
        (sourceFilter === 'web' && isWebDocument(doc)) ||
        (sourceFilter === 'local' && !isWebDocument(doc));
      const matchesBaseCollection = !baseCollectionDocIds || baseCollectionDocIds.has(doc.id);

      return matchesQuery && matchesType && matchesSource && matchesBaseCollection;
    });

    lastDocuments = documents;
    lastCustomCollections = state.customCollections;
    lastSavedViews = state.savedViews;
    lastActiveSavedViewId = state.activeSavedViewId;
    lastQuery = query;
    lastFilter = normalizedFilter;
    lastSourceFilter = sourceFilter;
    lastContentMatches = state.contentSearchMatches;
    lastResult = filtered;
    return lastResult;
  };
};

export const selectFilteredDocuments = buildFilteredDocumentsSelector();

const buildGroupedDocumentsSelector = () => {
  let lastFiltered: DocumentMetadata[] | null = null;
  let lastGroupByDate = true;
  let lastResult: ListItem[] = [];

  return (state: FileBrowserStore): ListItem[] => {
    const filtered = selectFilteredDocuments(state);

    if (filtered === lastFiltered && state.groupByDate === lastGroupByDate) {
      return lastResult;
    }

    if (!state.groupByDate) {
      lastFiltered = filtered;
      lastGroupByDate = false;
      lastResult = filtered.map(doc => ({ type: 'doc' as const, data: doc }));
      return lastResult;
    }

    const grouped = groupBy(filtered, doc => getDateGroup(doc.modifiedAt));
    const items: ListItem[] = [];
    const groupOrder: DateGroup[] = ['Today', 'Yesterday', 'This Week', 'Last Week', 'This Month', 'Older'];

    for (const label of groupOrder) {
      const docs = grouped[label];
      if (docs && docs.length > 0) {
        items.push({ type: 'header', label });
        docs.forEach(doc => items.push({ type: 'doc', data: doc }));
      }
    }

    lastFiltered = filtered;
    lastGroupByDate = true;
    lastResult = items;
    return lastResult;
  };
};

export const selectGroupedDocuments = buildGroupedDocumentsSelector();

export const selectDensity = (state: FileBrowserStore) => state.density;
export const selectGroupByDate = (state: FileBrowserStore) => state.groupByDate;
export const selectContentSearchLoading = (state: FileBrowserStore) => state.isContentSearchLoading;
