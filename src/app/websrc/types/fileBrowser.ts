/**
 * File Browser Type Definitions
 *
 * Comprehensive type system for the file browser feature
 */

export type ViewMode = 'tree' | 'list' | 'grid';
export type SortField = 'name' | 'size' | 'modified' | 'type';
export type SortOrder = 'asc' | 'desc';
export type SourceFilter = 'all' | 'local' | 'web';
export type ListColumnKey = 'words' | 'modified' | 'type';

export interface ListColumnVisibility {
  words: boolean;
  modified: boolean;
  type: boolean;
}

export interface SavedLibraryView {
  id: string;
  name: string;
  baseCollectionId: string | null;
  viewMode: ViewMode;
  density: Density;
  sortField: SortField;
  sortOrder: SortOrder;
  filterByType: string | null;
  filterBySource: SourceFilter;
  groupByDate: boolean;
  searchQuery: string;
  listColumns: ListColumnVisibility;
  createdAt: string;
  updatedAt: string;
}

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

// Legacy FileNode for backward compatibility with FileIcon/FileTypeBadge
export interface FileNode {
  id: string;
  name: string;
  path: string;
  type: 'file' | 'directory';
  size: number;
  modified: string;
  created?: string;
  extension?: string;
  mimeType?: string;
  isIndexed: boolean;
  children?: FileNode[];
  isExpanded?: boolean;
  depth?: number;
  parentPath?: string;
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

export type Density = 'compact' | 'comfortable' | 'spacious';

export interface FileBrowserActions {
  // View management
  setViewMode: (_mode: ViewMode) => void;
  setDensity: (_density: Density) => void;
  toggleGroupByDate: () => void;

  // Selection
  selectFile: (_fileId: string) => void;
  deselectFile: (_fileId: string) => void;
  toggleSelection: (_fileId: string) => void;
  selectAll: () => void;
  clearSelection: () => void;

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
  setListColumnVisibility: (_updates: Partial<ListColumnVisibility>) => void;
  toggleListColumnVisibility: (_column: ListColumnKey) => void;
  createSavedView: (_name: string) => string | null;
  applySavedView: (_viewId: string) => void;
  updateSavedView: (
    _viewId: string,
    _updates: Partial<Omit<SavedLibraryView, 'id' | 'createdAt' | 'updatedAt'>>
  ) => void;
  captureSavedViewState: (_viewId: string) => void;
  deleteSavedView: (_viewId: string) => void;
  clearActiveSavedView: () => void;
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
  reconcileLocalSources: (_connections: SourceConnection[]) => void;
  upsertSourceConnections: (_connections: SourceConnection[]) => void;
  updateSourceConnection: (_sourceId: string, _updates: Partial<SourceConnection>) => void;
  deleteSourceConnection: (_sourceId: string) => void;

  // Data operations
  loadFiles: () => Promise<void>;
  refreshFiles: () => Promise<void>;

  // Context menu
  openContextMenu: (_position: { x: number; y: number }, _file: FileNode) => void;
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

// Get file extension
export function getFileExtension(filename: string): string {
  const parts = filename.split('.');
  return parts.length > 1 ? parts[parts.length - 1].toLowerCase() : '';
}

// Get file icon name
export function getFileIconName(file: FileNode): string {
  if (file.type === 'directory') {
    return file.isExpanded ? FILE_TYPE_ICONS['folder-open'] : FILE_TYPE_ICONS['folder'];
  }

  const ext = file.extension || getFileExtension(file.name);
  return FILE_TYPE_ICONS[ext] || FILE_TYPE_ICONS['default'];
}
