/**
 * File Browser Store - Zustand Implementation
 *
 * Manages file browser state with efficient subscriptions
 * Uses Zustand for performance and simplicity
 */

import { create } from 'zustand';

import { typeBucket } from '@/features/files/model/documentMetadata';
import {
  type CustomCollection,
  type FileBrowserActions,
  type LibraryScope,
  type SavedSearchPreset,
  type SourceConnection,
  type SourceFilter,
  type ViewMode,
  type SortField,
  type SortOrder,
  type DocumentMetadata,
} from '@/types/fileBrowser';

interface FileBrowserState {
  // View configuration
  viewMode: ViewMode;
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
  savedSearches: SavedSearchPreset[];
  activeSavedSearchId: string | null;
  sourceConnections: SourceConnection[];

  // Scope
  scope: LibraryScope;

  // Selection
  selectedDocumentIds: Set<string>;

  // Neighborhood panel (pure UI preference)
  focusedDocumentId: string | null;
  isNeighborhoodOpen: boolean;

  // Context menu
  contextMenuPosition: { x: number; y: number } | null;
  contextMenuDocument: DocumentMetadata | null;
}

interface FileBrowserStore extends FileBrowserState, FileBrowserActions {}

export const sortLibraryDocuments = (
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

export interface LibraryFilterState {
  searchQuery: string;
  filterByType: string | null;
  filterBySource: SourceFilter;
  contentSearchMatches: Set<string>;
  customCollections: CustomCollection[];
  /**
   * Themes from the most recent clustering run, for `scope.kind === 'theme'`.
   * Optional because themes live in React Query, not in this store — callers
   * that never scope to a theme (and the store's own tests) omit it.
   */
  themes?: Array<{ id: string; memberDocumentIds: string[] }>;
  scope: LibraryScope;
}

const normalizeFolderPath = (value: string): string => {
  const normalized = value.replace(/\\/g, '/').replace(/\/+$/, '');
  return normalized.toLowerCase();
};

const isInsideFolder = (filePath: string, folderPath: string): boolean => {
  const file = filePath.replace(/\\/g, '/').toLowerCase();
  const folder = normalizeFolderPath(folderPath);
  return folder.length > 0 && file.startsWith(`${folder}/`);
};

export const filterLibraryDocuments = (
  documents: DocumentMetadata[],
  state: LibraryFilterState
): DocumentMetadata[] => {
  const query = normalizeToken(state.searchQuery);
  const normalizedFilter = state.filterByType ? normalizeToken(state.filterByType) : null;

  let scopedDocIds: Set<string> | null = null;
  if (state.scope.kind === 'collection') {
    const inScope = new Set<string>([
      state.scope.id,
      ...collectDescendantCollectionIds(state.customCollections, state.scope.id),
    ]);
    scopedDocIds = new Set<string>();
    for (const collection of state.customCollections) {
      if (inScope.has(collection.id)) {
        for (const docId of collection.documentIds) {
          scopedDocIds.add(docId);
        }
      }
    }
  }
  if (state.scope.kind === 'theme') {
    const scopeId = state.scope.id;
    const theme = state.themes?.find((candidate) => candidate.id === scopeId);
    scopedDocIds = new Set<string>(theme?.memberDocumentIds ?? []);
  }
  const scopedFolder = state.scope.kind === 'folder' ? state.scope.path : null;

  return documents.filter(doc => {
    const normalizedCategory = normalizeToken(doc.category).replace(/[_-]+/g, ' ');
    const normalizedFileType = normalizeToken(doc.fileType);
    const normalizedFileName = normalizeToken(doc.fileName);
    const matchesQuery = query
      ? normalizedFileName.includes(query) || state.contentSearchMatches.has(doc.id)
      : true;
    const matchesType = normalizedFilter
      ? normalizeToken(typeBucket(doc)) === normalizedFilter ||
        normalizedCategory.includes(normalizedFilter) ||
        normalizedFileType === normalizedFilter ||
        (normalizedFilter === 'web' && normalizedCategory.includes('web article'))
      : true;
    const matchesSource =
      state.filterBySource === 'all' ||
      (state.filterBySource === 'web' && isWebDocument(doc)) ||
      (state.filterBySource === 'local' && !isWebDocument(doc));
    const matchesScope =
      (!scopedDocIds || scopedDocIds.has(doc.id)) &&
      (!scopedFolder || isInsideFolder(doc.filePath, scopedFolder));
    return matchesQuery && matchesType && matchesSource && matchesScope;
  });
};
const SOURCE_CONNECTIONS_STORAGE_KEY = 'lattice:file-browser-source-connections:v1';
const SAVED_SEARCHES_STORAGE_KEY = 'lattice:file-browser-saved-searches:v1';

const sanitizeName = (value: string): string => value.trim().replace(/\s+/g, ' ');
const buildUniqueName = (candidate: string, existingNames: string[]): string => {
  const normalizedExisting = new Set(existingNames.map((name) => normalizeToken(name)));
  const name = sanitizeName(candidate) || 'Untitled';
  if (!normalizedExisting.has(normalizeToken(name))) return name;
  let index = 2;
  while (normalizedExisting.has(normalizeToken(`${name} (${index})`))) index += 1;
  return `${name} (${index})`;
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
    name: sanitizeName(candidate.name),
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
    !['web_import', 'dropbox', 'google_drive', 'onedrive', 'icloud'].includes(candidate.provider) ||
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
    const clientOnlyConnections = connections.filter(connection => connection.provider !== 'local_folder');
    window.localStorage.setItem(SOURCE_CONNECTIONS_STORAGE_KEY, JSON.stringify(clientOnlyConnections));
  } catch {
    // Ignore persistence failures; in-memory state remains valid
  }
};

const sortSourceConnections = (connections: SourceConnection[]): SourceConnection[] =>
  [...connections].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));

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

  createSavedSearch: (name: string, options) => {
    const normalizedName = sanitizeName(name);
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
        typeof updates.name === 'string' ? sanitizeName(updates.name) : target.name;
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

  createSourceConnection: (connection) => {
    if (connection.provider === 'local_folder') {
      throw new Error('Local folders are backend-owned and cannot be stored as client connections.');
    }

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

  updateSourceConnection: (sourceId: string, updates: Partial<SourceConnection>) => {
    set((state) => {
      const sourceConnections = state.sourceConnections.map((connection) =>
        connection.id === sourceId && connection.provider !== 'local_folder'
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
