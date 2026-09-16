/**
 * File Browser Type Definitions
 *
 * Comprehensive type system for the file browser feature
 */

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

export interface CustomCollection {
  id: string;
  name: string;
  kind: 'manual' | 'snapshot';
  parentId: string | null;
  documentIds: string[];
  createdAt: string;
  updatedAt: string;
}

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
  customCollections: CustomCollection[];
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
  createCustomCollection: (_name: string, _parentCollectionId?: string | null) => string | null;
  createSnapshotCollection: (
    _name: string,
    _documentIds: string[],
    _parentCollectionId?: string | null
  ) => string | null;
  renameCustomCollection: (_collectionId: string, _name: string) => void;
  updateCustomCollection: (
    _collectionId: string,
    _updates: Partial<Pick<CustomCollection, 'name' | 'parentId'>>
  ) => boolean;
  deleteCustomCollection: (_collectionId: string) => void;
  addDocumentsToCustomCollection: (_collectionId: string, _documentIds: string[]) => void;
  removeDocumentsFromCustomCollection: (_collectionId: string, _documentIds: string[]) => void;
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

// File type icons mapping
export const FILE_TYPE_ICONS: Record<string, string> = {
  // Documents
  pdf: 'FileText',
  doc: 'FileText',
  docx: 'FileText',
  txt: 'FileText',
  md: 'FileText',
  markdown: 'FileText',

  // Images
  jpg: 'Image',
  jpeg: 'Image',
  png: 'Image',
  gif: 'Image',
  svg: 'Image',
  webp: 'Image',

  // Code
  js: 'FileCode',
  jsx: 'FileCode',
  ts: 'FileCode',
  tsx: 'FileCode',
  py: 'FileCode',
  rs: 'FileCode',
  go: 'FileCode',
  java: 'FileCode',
  cpp: 'FileCode',
  c: 'FileCode',
  html: 'FileCode',
  css: 'FileCode',
  json: 'FileCode',
  xml: 'FileCode',

  // Archives
  zip: 'FileArchive',
  rar: 'FileArchive',
  tar: 'FileArchive',
  gz: 'FileArchive',
  '7z': 'FileArchive',

  // Video
  mp4: 'FileVideo',
  mov: 'FileVideo',
  avi: 'FileVideo',
  mkv: 'FileVideo',
  webm: 'FileVideo',

  // Audio
  mp3: 'FileAudio',
  wav: 'FileAudio',
  ogg: 'FileAudio',
  flac: 'FileAudio',

  // Spreadsheets
  xlsx: 'FileSpreadsheet',
  xls: 'FileSpreadsheet',
  csv: 'FileSpreadsheet',

  // Presentations
  pptx: 'FilePresentation',
  ppt: 'FilePresentation',

  // Default
  default: 'File',
  folder: 'Folder',
  'folder-open': 'FolderOpen',
};

// File size formatting
export function formatFileSize(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(2))} ${sizes[i]}`;
}

// File date formatting
export function formatFileDate(dateString: string): string {
  const date = new Date(dateString);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

  if (diffDays === 0) {
    return 'Today';
  } else if (diffDays === 1) {
    return 'Yesterday';
  } else if (diffDays < 7) {
    return `${diffDays} days ago`;
  } else {
    return date.toLocaleDateString();
  }
}

export function getFileExtension(filename: string): string {
  const parts = filename.split('.');
  return parts.length > 1 ? parts[parts.length - 1].toLowerCase() : '';
}
