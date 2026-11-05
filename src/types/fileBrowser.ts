/**
 * File Browser Type Definitions
 *
 * Comprehensive type system for the file browser feature
 */

import type { CustomCollectionDto } from '../lib/bindings';

export type ViewMode = 'tree' | 'list' | 'grid';
export type SortField = 'name' | 'size' | 'modified' | 'type';
export type SortOrder = 'asc' | 'desc';
export type SourceFilter = 'all' | 'local' | 'web';

/** What the document list is currently scoped to. */
export type LibraryScope =
  | { kind: 'all' }
  | { kind: 'folder'; path: string }
  | { kind: 'collection'; id: string }
  /** One automatic theme from the most recent clustering run. */
  | { kind: 'theme'; id: string };

export interface SavedSearchPreset {
  id: string;
  name: string;
  query: string;
  filterByType: string | null;
  filterBySource: SourceFilter;
  pinned: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface DocumentMetadata {
  id: string;
  fileName: string;
  filePath: string;
  fileType: string;
  category: string;
  language: string;
  modifiedAt: string;
  indexedAt: string;
  wordCount: number;
}

export type CustomCollection = CustomCollectionDto;

export type SourceProvider =
  | 'local_folder'
  | 'web_import'
  | 'dropbox'
  | 'google_drive'
  | 'onedrive'
  | 'icloud';

export type SourceMode = 'referenced' | 'managed';
export type SourceHealth = 'healthy' | 'attention' | 'error' | 'paused';

export interface SourceConnection {
  id: string;
  name: string;
  provider: SourceProvider;
  mode: SourceMode;
  health: SourceHealth;
  enabled: boolean;
  path?: string;
  lastSyncedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface FileBrowserState {
  // View configuration
  viewMode: ViewMode;

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

  /** The row the neighborhood panel is about — last clicked or opened. */
  focusedDocumentId: string | null;
  /** Whether the right rail is showing. A display preference, not backend state. */
  isNeighborhoodOpen: boolean;

  // Context menu
  contextMenuPosition: { x: number; y: number } | null;
  contextMenuDocument: DocumentMetadata | null;
}

export interface FileBrowserActions {
  // View management
  setViewMode: (_mode: ViewMode) => void;
  toggleGroupByDate: () => void;
  setScope: (_scope: LibraryScope) => void;

  // Selection
  selectFile: (_fileId: string) => void;
  deselectFile: (_fileId: string) => void;
  toggleSelection: (_fileId: string) => void;
  selectAll: (_fileIds: Iterable<string>) => void;
  clearSelection: () => void;

  // Neighborhood panel (pure UI preference)
  setFocusedDocument: (_documentId: string | null) => void;
  toggleNeighborhood: () => void;

  // Sorting and filtering
  setSortField: (_field: SortField) => void;
  setSortOrder: (_order: SortOrder) => void;
  toggleSortOrder: () => void;
  setSearchQuery: (_query: string) => void;
  setFilterByType: (_type: string | null) => void;
  setFilterBySource: (_source: SourceFilter) => void;
  setContentSearchMatches: (_matches: Iterable<string>) => void;
  setContentSearchLoading: (_isLoading: boolean) => void;
  clearContentSearch: () => void;
  createSavedSearch: (_name: string, _options?: { pinned?: boolean }) => string | null;
  applySavedSearch: (_searchId: string) => void;
  updateSavedSearch: (
    _searchId: string,
    _updates: Partial<Pick<SavedSearchPreset, 'name' | 'pinned'>>
  ) => void;
  reorderSavedSearches: (_orderedIds: string[]) => void;
  duplicateSavedSearch: (_searchId: string) => string | null;
  deleteSavedSearch: (_searchId: string) => void;
  clearActiveSavedSearch: () => void;
  createSourceConnection: (_connection: Omit<SourceConnection, 'id' | 'createdAt' | 'updatedAt'>) => string;
  updateSourceConnection: (_sourceId: string, _updates: Partial<SourceConnection>) => void;
  deleteSourceConnection: (_sourceId: string) => void;

  // Context menu
  openContextMenu: (_position: { x: number; y: number }, _document: DocumentMetadata) => void;
  closeContextMenu: () => void;
}

export interface FileAction {
  id: string;
  label: string;
  icon?: React.ReactNode;
  action: () => void | Promise<void>;
  disabled?: boolean;
  separator?: boolean;
  dangerous?: boolean;
}

export function getFileExtension(filename: string): string {
  const parts = filename.split('.');
  return parts.length > 1 ? parts[parts.length - 1].toLowerCase() : '';
}
