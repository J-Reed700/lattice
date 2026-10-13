import {
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

export interface FileBrowserStoreState {
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

export interface FileBrowserStore extends FileBrowserStoreState, FileBrowserActions {}
