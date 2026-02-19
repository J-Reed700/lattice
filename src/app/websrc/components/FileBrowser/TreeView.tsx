/**
 * Library View (Tree Mode)
 *
 * Collections-first organization with separate source management.
 * This avoids exposing raw filesystem roots as primary navigation.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { useVirtualizer } from '@tanstack/react-virtual';
import {
  ArrowDown,
  ArrowUp,
  ChevronDown,
  ChevronRight,
  Check,
  Clock3,
  Copy,
  FolderPlus,
  Edit3,
  ExternalLink,
  FileArchive,
  FileCode2,
  FileText,
  FolderKanban,
  Globe2,
  HardDrive,
  Image,
  Layers,
  Link,
  Loader2,
  Pin,
  PinOff,
  Plus,
  RefreshCw,
  Trash2,
  X,
} from 'lucide-react';

import { FileIcon } from './FileIcon';
import { FileTypeBadge } from './FileTypeBadge';
import VaultAPI from '../../lib/api';
import { useFileBrowserStore, selectFilteredDocuments } from '../../stores/fileBrowserStore';
import { toast } from '../../stores/toastStore';
import {
  type CustomCollection,
  type Density,
  type DocumentMetadata,
  type ListColumnVisibility,
  type SortField,
  type SortOrder,
  type SourceConnection,
  type SourceFilter,
  type SourceProvider,
  type ViewMode,
} from '../../types/fileBrowser';
import { formatRelativeTime } from '../../utils/dateUtils';
import { FolderList } from '../FolderList';
import { FileBrowserSkeleton } from '../Skeleton';
import { Checkbox } from '../ui/Checkbox';

const COLLECTION_ALL = 'collection:all';
const COLLECTION_RECENT = 'collection:recent';
const COLLECTION_WEB = 'collection:web';
const COLLECTION_LOCAL = 'collection:local';
const COLLECTION_DOCS = 'collection:docs';
const COLLECTION_MEDIA = 'collection:media';
const COLLECTION_CODE = 'collection:code';
const COLLECTION_ARCHIVES = 'collection:archives';

const WEB_ARCHIVE_MARKER = '/.recall/web-archive/';
const ROW_HEIGHT = 54;

const MEDIA_EXTENSIONS = new Set([
  'jpg',
  'jpeg',
  'png',
  'gif',
  'webp',
  'svg',
  'bmp',
  'ico',
  'mp3',
  'wav',
  'flac',
  'ogg',
  'm4a',
  'mp4',
  'mkv',
  'mov',
  'avi',
  'webm',
]);

const CODE_EXTENSIONS = new Set([
  'js',
  'jsx',
  'ts',
  'tsx',
  'py',
  'rs',
  'go',
  'java',
  'cpp',
  'c',
  'h',
  'hpp',
  'css',
  'scss',
  'html',
  'json',
  'yaml',
  'yml',
  'xml',
  'toml',
  'sql',
  'sh',
  'bash',
]);

const ARCHIVE_EXTENSIONS = new Set(['zip', 'rar', '7z', 'tar', 'gz', 'bz2', 'xz']);

type LeftMode = 'collections' | 'presets' | 'sources';

interface TreeViewProps {
  onFileOpen?: (_doc: DocumentMetadata) => void;
  onContextMenu?: (_event: React.MouseEvent, _doc: DocumentMetadata) => void;
  onRename?: (_doc: DocumentMetadata) => void;
  onDelete?: (_doc: DocumentMetadata) => void;
}

interface CollectionDef {
  id: string;
  name: string;
  description: string;
  icon: React.ComponentType<{ className?: string }>;
  docIds: Set<string>;
  badge?: 'Smart' | 'Project' | 'Custom' | 'Frozen' | 'Source';
}

interface SavedViewDraft {
  name: string;
  baseCollectionId: string | null;
  viewMode: ViewMode;
  density: Density;
  sortField: SortField;
  sortOrder: SortOrder;
  filterByType: string;
  filterBySource: SourceFilter;
  groupByDate: boolean;
  searchQuery: string;
  listColumns: ListColumnVisibility;
}

const isHttpUrl = (value: string): boolean => /^https?:\/\//i.test(value);

const normalizePath = (value: string): string => value.replace(/\\/g, '/');
const normalizePathPrefix = (value: string): string =>
  normalizePath(value).replace(/\/+$/, '');

const isWebDocument = (doc: DocumentMetadata): boolean => {
  if (isHttpUrl(doc.filePath)) {
    return true;
  }

  if (doc.filePath.includes(WEB_ARCHIVE_MARKER)) {
    return true;
  }

  const category = doc.category.toLowerCase().replace(/[_-]+/g, ' ').trim();
  return category.includes('web');
};

const extractProjectName = (path: string): string => {
  const normalized = normalizePath(path);
  const segments = normalized.split('/').filter(Boolean);

  if (segments.length === 0) {
    return 'Library';
  }

  if (segments[0] === 'Users' && segments.length >= 3) {
    if (['Documents', 'Desktop', 'Downloads', 'Code'].includes(segments[2]) && segments.length >= 4) {
      return segments[3];
    }
    return segments[2];
  }

  if (segments[0] === 'home' && segments.length >= 3) {
    if (['documents', 'desktop', 'downloads', 'code'].includes(segments[2].toLowerCase()) && segments.length >= 4) {
      return segments[3];
    }
    return segments[2];
  }

  return segments[0];
};

const isRecentDocument = (doc: DocumentMetadata): boolean => {
  const now = Date.now();
  const modifiedAt = new Date(doc.modifiedAt).getTime();
  const indexedAt = new Date(doc.indexedAt).getTime();
  const candidate = Number.isFinite(indexedAt) ? Math.max(modifiedAt, indexedAt) : modifiedAt;
  if (!Number.isFinite(candidate)) {
    return false;
  }

  const fourteenDaysMs = 14 * 24 * 60 * 60 * 1000;
  return now - candidate <= fourteenDaysMs;
};

const buildCollectionModel = (documents: DocumentMetadata[]): {
  collections: CollectionDef[];
  projects: CollectionDef[];
} => {
  const allIds = new Set(documents.map(doc => doc.id));
  const recentIds = new Set(documents.filter(isRecentDocument).map(doc => doc.id));
  const webIds = new Set(documents.filter(isWebDocument).map(doc => doc.id));
  const localIds = new Set(documents.filter(doc => !isWebDocument(doc)).map(doc => doc.id));

  const docsIds = new Set(
    documents
      .filter((doc) => {
        const ext = (doc.fileType ?? '').toLowerCase().trim();
        return !MEDIA_EXTENSIONS.has(ext) && !CODE_EXTENSIONS.has(ext) && !ARCHIVE_EXTENSIONS.has(ext);
      })
      .map(doc => doc.id)
  );

  const mediaIds = new Set(
    documents
      .filter(doc => MEDIA_EXTENSIONS.has((doc.fileType ?? '').toLowerCase().trim()))
      .map(doc => doc.id)
  );

  const codeIds = new Set(
    documents
      .filter(doc => CODE_EXTENSIONS.has((doc.fileType ?? '').toLowerCase().trim()))
      .map(doc => doc.id)
  );

  const archiveIds = new Set(
    documents
      .filter(doc => ARCHIVE_EXTENSIONS.has((doc.fileType ?? '').toLowerCase().trim()))
      .map(doc => doc.id)
  );

  const smartCollections: CollectionDef[] = [
    {
      id: COLLECTION_ALL,
      name: 'All Documents',
      description: 'Everything in your knowledge library.',
      icon: Layers,
      docIds: allIds,
      badge: 'Smart',
    },
    {
      id: COLLECTION_RECENT,
      name: 'Recently Active',
      description: 'Indexed or modified in the last 14 days.',
      icon: Clock3,
      docIds: recentIds,
      badge: 'Smart',
    },
    {
      id: COLLECTION_WEB,
      name: 'Web Captures',
      description: 'Imported web pages and archived content.',
      icon: Globe2,
      docIds: webIds,
      badge: 'Smart',
    },
    {
      id: COLLECTION_LOCAL,
      name: 'Local References',
      description: 'Files backed by local folders.',
      icon: HardDrive,
      docIds: localIds,
      badge: 'Smart',
    },
    {
      id: COLLECTION_DOCS,
      name: 'Documents',
      description: 'Text-first files like notes and PDFs.',
      icon: FileText,
      docIds: docsIds,
      badge: 'Smart',
    },
    {
      id: COLLECTION_MEDIA,
      name: 'Media',
      description: 'Images, audio, and video assets.',
      icon: Image,
      docIds: mediaIds,
      badge: 'Smart',
    },
    {
      id: COLLECTION_CODE,
      name: 'Code & Data',
      description: 'Source files, configs, and structured data.',
      icon: FileCode2,
      docIds: codeIds,
      badge: 'Smart',
    },
    {
      id: COLLECTION_ARCHIVES,
      name: 'Archives',
      description: 'Compressed packages and bundles.',
      icon: FileArchive,
      docIds: archiveIds,
      badge: 'Smart',
    },
  ];

  const projectMap = new Map<string, Set<string>>();

  for (const doc of documents) {
    if (isWebDocument(doc)) {
      continue;
    }

    const projectName = extractProjectName(doc.filePath);
    const key = projectName.trim() || 'Library';

    if (!projectMap.has(key)) {
      projectMap.set(key, new Set());
    }

    projectMap.get(key)?.add(doc.id);
  }

  const projects = Array.from(projectMap.entries())
    .map(([name, ids]) => ({
      id: `project:${name.toLowerCase()}`,
      name,
      description: 'Path-derived project cluster.',
      icon: FolderKanban,
      docIds: ids,
      badge: 'Project' as const,
    }))
    .sort((a, b) => b.docIds.size - a.docIds.size)
    .slice(0, 20);

  return {
    collections: smartCollections,
    projects,
  };
};

const sourceSummary = (documents: DocumentMetadata[]) => {
  let local = 0;
  let web = 0;

  for (const doc of documents) {
    if (isWebDocument(doc)) {
      web += 1;
    } else {
      local += 1;
    }
  }

  return { local, web };
};

const matchDocumentToSource = (doc: DocumentMetadata, source: SourceConnection): boolean => {
  if (source.provider === 'local_folder') {
    if (!source.path) {
      return false;
    }

    if (isWebDocument(doc)) {
      return false;
    }

    const docPath = normalizePath(doc.filePath);
    const sourcePath = normalizePathPrefix(source.path);
    return docPath === sourcePath || docPath.startsWith(`${sourcePath}/`);
  }

  if (source.provider === 'web_import') {
    return isWebDocument(doc);
  }

  return false;
};

export const TreeView = ({ onFileOpen, onContextMenu, onRename, onDelete }: TreeViewProps) => {
  const allDocuments = useFileBrowserStore(state => state.documents);
  const filteredDocuments = useFileBrowserStore(selectFilteredDocuments);
  const searchQuery = useFileBrowserStore(state => state.searchQuery);
  const viewMode = useFileBrowserStore(state => state.viewMode);
  const density = useFileBrowserStore(state => state.density);
  const sortField = useFileBrowserStore(state => state.sortField);
  const sortOrder = useFileBrowserStore(state => state.sortOrder);
  const filterByType = useFileBrowserStore(state => state.filterByType);
  const filterBySource = useFileBrowserStore(state => state.filterBySource);
  const groupByDate = useFileBrowserStore(state => state.groupByDate);
  const selectedDocumentIds = useFileBrowserStore(state => state.selectedDocumentIds);
  const customCollections = useFileBrowserStore(state => state.customCollections);
  const savedViews = useFileBrowserStore(state => state.savedViews);
  const activeSavedViewId = useFileBrowserStore(state => state.activeSavedViewId);
  const savedSearches = useFileBrowserStore(state => state.savedSearches);
  const activeSavedSearchId = useFileBrowserStore(state => state.activeSavedSearchId);
  const listColumns = useFileBrowserStore(state => state.listColumns);
  const sourceConnections = useFileBrowserStore(state => state.sourceConnections);
  const createCustomCollection = useFileBrowserStore(state => state.createCustomCollection);
  const createSnapshotCollection = useFileBrowserStore(state => state.createSnapshotCollection);
  const createSavedView = useFileBrowserStore(state => state.createSavedView);
  const applySavedView = useFileBrowserStore(state => state.applySavedView);
  const updateSavedView = useFileBrowserStore(state => state.updateSavedView);
  const captureSavedViewState = useFileBrowserStore(state => state.captureSavedViewState);
  const deleteSavedView = useFileBrowserStore(state => state.deleteSavedView);
  const createSavedSearch = useFileBrowserStore(state => state.createSavedSearch);
  const applySavedSearch = useFileBrowserStore(state => state.applySavedSearch);
  const updateSavedSearch = useFileBrowserStore(state => state.updateSavedSearch);
  const reorderSavedSearches = useFileBrowserStore(state => state.reorderSavedSearches);
  const duplicateSavedSearch = useFileBrowserStore(state => state.duplicateSavedSearch);
  const deleteSavedSearch = useFileBrowserStore(state => state.deleteSavedSearch);
  const renameCustomCollection = useFileBrowserStore(state => state.renameCustomCollection);
  const updateCustomCollection = useFileBrowserStore(state => state.updateCustomCollection);
  const deleteCustomCollection = useFileBrowserStore(state => state.deleteCustomCollection);
  const addDocumentsToCustomCollection = useFileBrowserStore(state => state.addDocumentsToCustomCollection);
  const removeDocumentsFromCustomCollection = useFileBrowserStore(state => state.removeDocumentsFromCustomCollection);
  const createSourceConnection = useFileBrowserStore(state => state.createSourceConnection);
  const reconcileLocalSources = useFileBrowserStore(state => state.reconcileLocalSources);
  const updateSourceConnection = useFileBrowserStore(state => state.updateSourceConnection);
  const deleteSourceConnection = useFileBrowserStore(state => state.deleteSourceConnection);
  const selectFile = useFileBrowserStore(state => state.selectFile);
  const toggleSelection = useFileBrowserStore(state => state.toggleSelection);
  const clearSelection = useFileBrowserStore(state => state.clearSelection);
  const isLoading = useFileBrowserStore(state => state.isLoading);
  const error = useFileBrowserStore(state => state.error);

  const [leftMode, setLeftMode] = useState<LeftMode>('collections');
  const [selectedCollectionId, setSelectedCollectionId] = useState(COLLECTION_ALL);
  const [lastSelectedIndex, setLastSelectedIndex] = useState(-1);
  const [newCollectionName, setNewCollectionName] = useState('');
  const [renamingCollectionId, setRenamingCollectionId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState('');
  const [collectionEditorName, setCollectionEditorName] = useState('');
  const [collectionEditorParentId, setCollectionEditorParentId] = useState<string | null>(null);
  const [newViewName, setNewViewName] = useState('');
  const [newSearchName, setNewSearchName] = useState('');
  const [renamingSearchId, setRenamingSearchId] = useState<string | null>(null);
  const [renameSearchValue, setRenameSearchValue] = useState('');
  const [selectedViewId, setSelectedViewId] = useState<string | null>(null);
  const [viewDraft, setViewDraft] = useState<SavedViewDraft | null>(null);
  const [collapsedCollectionIds, setCollapsedCollectionIds] = useState<Set<string>>(new Set());
  const [isSyncingSources, setIsSyncingSources] = useState(false);
  const [newSourceName, setNewSourceName] = useState('');
  const [newSourceProvider, setNewSourceProvider] = useState<SourceProvider>('dropbox');
  const [newSourceMode, setNewSourceMode] = useState<SourceConnection['mode']>('referenced');

  const { collections, projects } = useMemo(() => buildCollectionModel(filteredDocuments), [filteredDocuments]);
  const sources = useMemo(() => sourceSummary(filteredDocuments), [filteredDocuments]);
  const filteredDocIdSet = useMemo(() => new Set(filteredDocuments.map(doc => doc.id)), [filteredDocuments]);
  const selectedSavedView = useMemo(
    () =>
      savedViews.find((view) => view.id === (selectedViewId ?? activeSavedViewId ?? '')) ?? null,
    [activeSavedViewId, savedViews, selectedViewId]
  );

  useEffect(() => {
    if (!activeSavedViewId) {
      return;
    }
    setSelectedViewId(activeSavedViewId);
  }, [activeSavedViewId]);

  useEffect(() => {
    if (savedViews.length === 0) {
      setSelectedViewId(null);
      return;
    }

    if (selectedViewId && savedViews.some((view) => view.id === selectedViewId)) {
      return;
    }

    setSelectedViewId(savedViews[0]?.id ?? null);
  }, [savedViews, selectedViewId]);

  useEffect(() => {
    if (!selectedSavedView) {
      setViewDraft(null);
      return;
    }

    setViewDraft({
      name: selectedSavedView.name,
      baseCollectionId: selectedSavedView.baseCollectionId ?? null,
      viewMode: selectedSavedView.viewMode,
      density: selectedSavedView.density,
      sortField: selectedSavedView.sortField,
      sortOrder: selectedSavedView.sortOrder,
      filterByType: selectedSavedView.filterByType ?? '',
      filterBySource: selectedSavedView.filterBySource,
      groupByDate: selectedSavedView.groupByDate,
      searchQuery: selectedSavedView.searchQuery,
      listColumns: { ...selectedSavedView.listColumns },
    });
  }, [selectedSavedView]);

  const customCollectionDefs = useMemo((): CollectionDef[] => customCollections.map((collection): CollectionDef => ({
      id: collection.id,
      name: collection.name,
      description:
        collection.kind === 'snapshot'
          ? 'Frozen collection captured from a result set.'
          : 'Manually curated document set.',
      icon: FolderPlus,
      docIds: new Set(collection.documentIds.filter(docId => filteredDocIdSet.has(docId))),
      badge: collection.kind === 'snapshot' ? 'Frozen' : 'Custom',
    })), [customCollections, filteredDocIdSet]);
  const customCollectionCountById = useMemo(() => {
    const countMap = new Map<string, number>();
    for (const def of customCollectionDefs) {
      countMap.set(def.id, def.docIds.size);
    }
    return countMap;
  }, [customCollectionDefs]);
  const customCollectionsByParentId = useMemo(() => {
    const map = new Map<string | null, CustomCollection[]>();
    for (const collection of customCollections) {
      const key = collection.parentId ?? null;
      const bucket = map.get(key);
      if (bucket) {
        bucket.push(collection);
      } else {
        map.set(key, [collection]);
      }
    }

    for (const [, bucket] of map) {
      bucket.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
    }

    return map;
  }, [customCollections]);
  const customCollectionsById = useMemo(
    () => new Map(customCollections.map((collection) => [collection.id, collection])),
    [customCollections]
  );
  const customCollectionSwitchOptions = useMemo(() => {
    const options: Array<{
      id: string;
      label: string;
      count: number;
    }> = [];

    const walk = (parentId: string | null, depth: number) => {
      const children = customCollectionsByParentId.get(parentId) ?? [];
      for (const collection of children) {
        const prefix = depth > 0 ? `${'  '.repeat(depth)}└ ` : '';
        const suffix = collection.kind === 'snapshot' ? ' [frozen]' : '';
        options.push({
          id: collection.id,
          label: `${prefix}${collection.name}${suffix}`,
          count: customCollectionCountById.get(collection.id) ?? 0,
        });
        walk(collection.id, depth + 1);
      }
    };

    walk(null, 0);
    return options;
  }, [customCollectionCountById, customCollectionsByParentId]);
  const collectionSwitchOrder = useMemo(
    () => [
      COLLECTION_ALL,
      ...projects.map((project) => project.id),
      ...customCollectionSwitchOptions.map((collection) => collection.id),
    ],
    [customCollectionSwitchOptions, projects]
  );
  const switcherSelectedCollectionId = useMemo(
    () =>
      collectionSwitchOrder.includes(selectedCollectionId)
        ? selectedCollectionId
        : COLLECTION_ALL,
    [collectionSwitchOrder, selectedCollectionId]
  );

  const sourceCollectionDefs = useMemo((): CollectionDef[] => sourceConnections.map((source): CollectionDef => {
      const ids = new Set(
        filteredDocuments
          .filter(doc => matchDocumentToSource(doc, source))
          .map(doc => doc.id)
      );

      return {
        id: source.id,
        name: source.name,
        description: `Source: ${source.provider.replace(/_/g, ' ')}`,
        icon: source.provider === 'web_import' ? Globe2 : HardDrive,
        docIds: ids,
        badge: 'Source',
      };
    }), [filteredDocuments, sourceConnections]);

  const collectionMap = useMemo(() => {
    const map = new Map<string, CollectionDef>();
    collections.forEach(collection => map.set(collection.id, collection));
    projects.forEach(project => map.set(project.id, project));
    customCollectionDefs.forEach(collection => map.set(collection.id, collection));
    sourceCollectionDefs.forEach(collection => map.set(collection.id, collection));
    return map;
  }, [collections, customCollectionDefs, projects, sourceCollectionDefs]);

  useEffect(() => {
    if (!collectionMap.has(selectedCollectionId)) {
      setSelectedCollectionId(COLLECTION_ALL);
    }
  }, [collectionMap, selectedCollectionId]);

  const activeCollection = collectionMap.get(selectedCollectionId) ?? collections[0];
  const activeCustomCollection = useMemo(
    () => customCollections.find(collection => collection.id === selectedCollectionId) ?? null,
    [customCollections, selectedCollectionId]
  );
  const collectionEditorParentOptions = useMemo(() => {
    if (!activeCustomCollection) {
      return [];
    }

    const descendantIds = new Set<string>();
    const queue = [activeCustomCollection.id];
    while (queue.length > 0) {
      const current = queue.shift();
      if (!current) {
        continue;
      }

      for (const collection of customCollections) {
        if (collection.parentId === current && !descendantIds.has(collection.id)) {
          descendantIds.add(collection.id);
          queue.push(collection.id);
        }
      }
    }

    return customCollections.filter((collection) => {
      if (collection.id === activeCustomCollection.id) {
        return false;
      }
      if (descendantIds.has(collection.id)) {
        return false;
      }
      return collection.kind === 'manual';
    });
  }, [activeCustomCollection, customCollections]);

  useEffect(() => {
    if (!activeCustomCollection) {
      setCollectionEditorName('');
      setCollectionEditorParentId(null);
      return;
    }

    setCollectionEditorName(activeCustomCollection.name);
    setCollectionEditorParentId(activeCustomCollection.parentId ?? null);
  }, [activeCustomCollection]);

  const visibleDocuments = useMemo(() => {
    if (!activeCollection) {
      return filteredDocuments;
    }

    return filteredDocuments.filter(doc => activeCollection.docIds.has(doc.id));
  }, [activeCollection, filteredDocuments]);

  const documentsPanelRef = useRef<HTMLDivElement>(null);

  const documentVirtualizer = useVirtualizer({
    count: visibleDocuments.length,
    getScrollElement: () => documentsPanelRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 10,
  });

  const handleDocumentClick = useCallback((doc: DocumentMetadata, index: number, event: React.MouseEvent) => {
    if (event.metaKey || event.ctrlKey) {
      toggleSelection(doc.id);
      setLastSelectedIndex(index);
      return;
    }

    if (event.shiftKey && lastSelectedIndex >= 0) {
      const start = Math.min(lastSelectedIndex, index);
      const end = Math.max(lastSelectedIndex, index);
      clearSelection();
      visibleDocuments.slice(start, end + 1).forEach((item) => selectFile(item.id));
      return;
    }

    clearSelection();
    selectFile(doc.id);
    setLastSelectedIndex(index);
  }, [clearSelection, lastSelectedIndex, selectFile, toggleSelection, visibleDocuments]);

  const toggleSelectAllVisible = useCallback(() => {
    const allVisibleSelected =
      visibleDocuments.length > 0 && visibleDocuments.every(doc => selectedDocumentIds.has(doc.id));

    if (allVisibleSelected) {
      clearSelection();
      return;
    }

    clearSelection();
    visibleDocuments.forEach(doc => selectFile(doc.id));
  }, [clearSelection, selectFile, selectedDocumentIds, visibleDocuments]);

  const allVisibleSelected =
    visibleDocuments.length > 0 && visibleDocuments.every(doc => selectedDocumentIds.has(doc.id));
  const someVisibleSelected =
    visibleDocuments.some(doc => selectedDocumentIds.has(doc.id)) && !allVisibleSelected;
  const selectedIds = useMemo(() => Array.from(selectedDocumentIds), [selectedDocumentIds]);

  const selectedIdsInActiveCustomCollection = useMemo(() => {
    if (!activeCustomCollection) {
      return [];
    }
    return selectedIds.filter(docId => activeCustomCollection.documentIds.includes(docId));
  }, [activeCustomCollection, selectedIds]);

  const handleCreateCustomCollection = useCallback((parentId: string | null = null) => {
    const collectionId = createCustomCollection(newCollectionName, parentId);
    if (!collectionId) {
      toast.warning('Collection not created', {
        message: 'Name is empty, duplicate at this level, or parent is missing.',
      });
      return;
    }

    setSelectedCollectionId(collectionId);
    setNewCollectionName('');
    toast.success('Collection created');
  }, [createCustomCollection, newCollectionName]);

  const handleCreateSnapshotFromSelection = useCallback((parentId: string | null = null) => {
    if (selectedIds.length === 0) {
      toast.warning('Select documents first', {
        message: 'Choose one or more files to freeze into a collection.',
      });
      return;
    }

    const fallbackName = `Frozen ${new Date().toLocaleDateString()}`;
    const snapshotName = newCollectionName.trim() || fallbackName;
    const collectionId = createSnapshotCollection(snapshotName, selectedIds, parentId);
    if (!collectionId) {
      toast.warning('Frozen collection not created', {
        message: 'Name is duplicate at this level or parent is missing.',
      });
      return;
    }

    setSelectedCollectionId(collectionId);
    setNewCollectionName('');
    toast.success('Frozen collection created');
  }, [createSnapshotCollection, newCollectionName, selectedIds]);

  const handleStartRename = useCallback((collection: CustomCollection) => {
    setRenamingCollectionId(collection.id);
    setRenameValue(collection.name);
  }, []);

  const handleCommitRename = useCallback((collectionId: string) => {
    renameCustomCollection(collectionId, renameValue);
    setRenamingCollectionId(null);
    setRenameValue('');
  }, [renameCustomCollection, renameValue]);

  const handleSaveCollectionEditor = useCallback(() => {
    if (!activeCustomCollection) {
      return;
    }

    const updated = updateCustomCollection(activeCustomCollection.id, {
      name: collectionEditorName,
      parentId: collectionEditorParentId,
    });

    if (!updated) {
      toast.warning('Collection not updated', {
        message: 'Check name, parent selection, and duplicate names at the target level.',
      });
      return;
    }

    toast.success('Collection updated');
  }, [activeCustomCollection, collectionEditorName, collectionEditorParentId, updateCustomCollection]);

  const handleDeleteCustomCollection = useCallback((collectionId: string) => {
    deleteCustomCollection(collectionId);
    if (selectedCollectionId === collectionId) {
      setSelectedCollectionId(COLLECTION_ALL);
    }
    toast.success('Collection deleted');
  }, [deleteCustomCollection, selectedCollectionId]);

  const handleCycleCollection = useCallback((direction: 'prev' | 'next') => {
    if (collectionSwitchOrder.length <= 1) {
      return;
    }

    const currentIndex = collectionSwitchOrder.indexOf(switcherSelectedCollectionId);
    const normalizedIndex = currentIndex >= 0 ? currentIndex : 0;
    const delta = direction === 'next' ? 1 : -1;
    const nextIndex =
      (normalizedIndex + delta + collectionSwitchOrder.length) %
      collectionSwitchOrder.length;
    setSelectedCollectionId(collectionSwitchOrder[nextIndex]);
  }, [collectionSwitchOrder, switcherSelectedCollectionId]);

  const handleCreateSubcollection = useCallback((parentCollection: CustomCollection) => {
    if (parentCollection.kind === 'snapshot') {
      toast.warning('Frozen collections are immutable', {
        message: 'Create subfolders under a manual collection.',
      });
      return;
    }

    const siblingCollections = customCollections.filter(
      collection => (collection.parentId ?? null) === parentCollection.id
    );
    const siblingNameSet = new Set(
      siblingCollections.map(collection => collection.name.trim().toLowerCase())
    );

    let candidate = 'New Folder';
    let index = 2;
    while (siblingNameSet.has(candidate.toLowerCase())) {
      candidate = `New Folder ${index}`;
      index += 1;
    }

    const collectionId = createCustomCollection(candidate, parentCollection.id);
    if (!collectionId) {
      toast.warning('Subfolder not created');
      return;
    }

    setSelectedCollectionId(collectionId);
    toast.success(`Subfolder added in ${parentCollection.name}`);
  }, [createCustomCollection, customCollections]);

  const handleCreateSavedView = useCallback(() => {
    const viewId = createSavedView(newViewName);
    if (!viewId) {
      toast.warning('View not created', {
        message: 'Name is empty or already used.',
      });
      return;
    }

    setNewViewName('');
    setSelectedViewId(viewId);
    toast.success('View preset created');
  }, [createSavedView, newViewName]);

  const handleApplySavedView = useCallback((viewId: string) => {
    applySavedView(viewId);
    setSelectedViewId(viewId);
    toast.success('View preset applied');
  }, [applySavedView]);

  const handleCaptureSavedView = useCallback((viewId: string) => {
    captureSavedViewState(viewId);
    toast.success('View preset refreshed from current controls');
  }, [captureSavedViewState]);

  const handleDeleteSavedView = useCallback((viewId: string) => {
    deleteSavedView(viewId);
    if (selectedViewId === viewId) {
      setSelectedViewId(null);
    }
    toast.success('View preset deleted');
  }, [deleteSavedView, selectedViewId]);

  const handleCreateSavedSearch = useCallback(() => {
    const searchId = createSavedSearch(newSearchName);
    if (!searchId) {
      toast.warning('Saved search not created', {
        message: 'Set a query/filter first, and use a unique non-empty name.',
      });
      return;
    }

    setNewSearchName('');
    toast.success('Search preset created');
  }, [createSavedSearch, newSearchName]);

  const handleTogglePinnedSearch = useCallback((searchId: string, pinned: boolean) => {
    updateSavedSearch(searchId, { pinned: !pinned });
  }, [updateSavedSearch]);

  const handleStartRenameSavedSearch = useCallback((searchId: string, name: string) => {
    setRenamingSearchId(searchId);
    setRenameSearchValue(name);
  }, []);

  const handleCommitRenameSavedSearch = useCallback((searchId: string) => {
    const nextName = renameSearchValue.trim();
    if (!nextName) {
      toast.warning('Search name cannot be empty');
      return;
    }

    updateSavedSearch(searchId, { name: nextName });
    setRenamingSearchId(null);
    setRenameSearchValue('');
  }, [renameSearchValue, updateSavedSearch]);

  const handleMoveSavedSearch = useCallback((searchId: string, direction: 'up' | 'down') => {
    const currentIds = savedSearches.map((search) => search.id);
    const index = currentIds.indexOf(searchId);
    if (index < 0) {
      return;
    }

    const targetIndex = direction === 'up' ? index - 1 : index + 1;
    if (targetIndex < 0 || targetIndex >= currentIds.length) {
      return;
    }

    const nextIds = [...currentIds];
    const [moved] = nextIds.splice(index, 1);
    nextIds.splice(targetIndex, 0, moved);
    reorderSavedSearches(nextIds);
  }, [reorderSavedSearches, savedSearches]);

  const handleDuplicateSavedSearch = useCallback((searchId: string) => {
    const duplicateId = duplicateSavedSearch(searchId);
    if (!duplicateId) {
      toast.warning('Unable to duplicate saved search');
      return;
    }

    toast.success('Saved search duplicated');
  }, [duplicateSavedSearch]);

  const handleSaveViewDraft = useCallback(() => {
    if (!selectedSavedView || !viewDraft) {
      return;
    }

    updateSavedView(selectedSavedView.id, {
      name: viewDraft.name,
      baseCollectionId: viewDraft.baseCollectionId,
      viewMode: viewDraft.viewMode,
      density: viewDraft.density,
      sortField: viewDraft.sortField,
      sortOrder: viewDraft.sortOrder,
      filterByType: viewDraft.filterByType || null,
      filterBySource: viewDraft.filterBySource,
      groupByDate: viewDraft.groupByDate,
      searchQuery: viewDraft.searchQuery,
      listColumns: viewDraft.listColumns,
    });
    toast.success('View preset updated');
  }, [selectedSavedView, updateSavedView, viewDraft]);

  const handleLoadCurrentControlsIntoDraft = useCallback(() => {
    setViewDraft((current) => {
      if (!current) {
        return current;
      }

      return {
        ...current,
        baseCollectionId: current.baseCollectionId,
        viewMode,
        density,
        sortField,
        sortOrder,
        filterByType: filterByType ?? '',
        filterBySource,
        groupByDate,
        searchQuery,
        listColumns: { ...listColumns },
      };
    });
  }, [density, filterBySource, filterByType, groupByDate, listColumns, searchQuery, sortField, sortOrder, viewMode]);

  const toggleCollectionCollapsed = useCallback((collectionId: string) => {
    setCollapsedCollectionIds((previous) => {
      const next = new Set(previous);
      if (next.has(collectionId)) {
        next.delete(collectionId);
      } else {
        next.add(collectionId);
      }
      return next;
    });
  }, []);

  const handleAddSelectedToActiveCollection = useCallback(() => {
    if (!activeCustomCollection) {
      return;
    }
    addDocumentsToCustomCollection(activeCustomCollection.id, selectedIds);
    toast.success('Added selected documents');
  }, [activeCustomCollection, addDocumentsToCustomCollection, selectedIds]);

  const handleRemoveSelectedFromActiveCollection = useCallback(() => {
    if (!activeCustomCollection) {
      return;
    }
    removeDocumentsFromCustomCollection(activeCustomCollection.id, selectedIdsInActiveCustomCollection);
    toast.success('Removed from collection');
  }, [activeCustomCollection, removeDocumentsFromCustomCollection, selectedIdsInActiveCustomCollection]);

  const syncLocalSources = useCallback(async () => {
    setIsSyncingSources(true);
    const result = await VaultAPI.getIndexedFolders();
    if (!result.ok) {
      toast.error('Failed to sync sources', { message: result.error });
      setIsSyncingSources(false);
      return;
    }

    const now = new Date().toISOString();
    const existingConnections = useFileBrowserStore.getState().sourceConnections;
    const connections: SourceConnection[] = result.data.map((folder) => {
      const existing = existingConnections.find(connection => connection.id === `local:${folder.path}`);
      return {
        id: `local:${folder.path}`,
        name: folder.path.split(/[\\/]/).filter(Boolean).slice(-1)[0] || folder.path,
        provider: 'local_folder',
        mode: existing?.mode ?? 'referenced',
        health: folder.enabled ? 'healthy' : 'paused',
        enabled: folder.enabled,
        path: folder.path,
        lastSyncedAt: folder.last_scan,
        createdAt: existing?.createdAt ?? now,
        updatedAt: now,
      };
    });

    reconcileLocalSources(connections);
    setIsSyncingSources(false);
  }, [reconcileLocalSources]);

  useEffect(() => {
    void syncLocalSources();
  }, [syncLocalSources]);

  const handleCreateCloudSource = useCallback(() => {
    const trimmedName = newSourceName.trim();
    if (!trimmedName) {
      toast.warning('Source not created', { message: 'Enter a source name first.' });
      return;
    }

    createSourceConnection({
      name: trimmedName,
      provider: newSourceProvider,
      mode: newSourceMode,
      health: 'attention',
      enabled: false,
      lastSyncedAt: null,
    });
    setNewSourceName('');
    toast.success('Cloud source added', { message: 'Connection is pending authentication.' });
  }, [createSourceConnection, newSourceMode, newSourceName, newSourceProvider]);

  const renderCustomCollectionNode = useCallback((collection: CustomCollection, depth = 0): React.ReactNode => {
    const isActive = selectedCollectionId === collection.id;
    const filteredCount = customCollectionCountById.get(collection.id) ?? 0;
    const isRenaming = renamingCollectionId === collection.id;
    const isSnapshot = collection.kind === 'snapshot';
    const CollectionIcon = isSnapshot ? FileArchive : FolderKanban;
    const children = customCollectionsByParentId.get(collection.id) ?? [];
    const hasChildren = children.length > 0;
    const isCollapsed = collapsedCollectionIds.has(collection.id);
    const isExpanded = !isCollapsed;

    return (
      <div key={collection.id}>
        <div
          className={`group rounded-xl border px-2 py-1.5 transition-colors ${
            isActive
              ? 'border-[var(--accent-primary)]/35 bg-[var(--accent-light)]/45'
              : 'border-transparent hover:border-[var(--border-color)] hover:bg-[var(--surface-hover)]'
          }`}
          style={{ marginLeft: `${depth * 14}px` }}
        >
          <div className="flex items-center gap-2">
            <div
              role="button"
              tabIndex={0}
              onClick={() => setSelectedCollectionId(collection.id)}
              onKeyDown={(event) => {
                if (event.key === 'Enter' || event.key === ' ') {
                  event.preventDefault();
                  setSelectedCollectionId(collection.id);
                }
              }}
              className="flex min-w-0 flex-1 items-center gap-2 text-left"
            >
              {hasChildren ? (
                <button
                  type="button"
                  aria-label={isExpanded ? 'Collapse folder' : 'Expand folder'}
                  title={isExpanded ? 'Collapse' : 'Expand'}
                  onClick={(event) => {
                    event.preventDefault();
                    event.stopPropagation();
                    toggleCollectionCollapsed(collection.id);
                  }}
                  className="inline-flex h-4 w-4 items-center justify-center rounded text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
                >
                  {isExpanded ? (
                    <ChevronDown className="h-3.5 w-3.5" />
                  ) : (
                    <ChevronRight className="h-3.5 w-3.5" />
                  )}
                </button>
              ) : (
                <span className="inline-block h-4 w-4 shrink-0" />
              )}
              <CollectionIcon className="h-4 w-4 shrink-0 text-[var(--accent-primary)]" />
              {isRenaming ? (
                <input
                  type="text"
                  value={renameValue}
                  onChange={(event) => setRenameValue(event.target.value)}
                  className="h-7 w-full rounded border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                  onClick={(event) => event.stopPropagation()}
                />
              ) : (
                <span className="inline-flex min-w-0 items-center gap-1.5">
                  <span className="truncate text-sm font-medium text-[var(--text-primary)]">{collection.name}</span>
                  {isSnapshot && (
                    <span className="rounded-full border border-[var(--border-color)] bg-[var(--surface-elevated)] px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-[var(--text-tertiary)]">
                      Frozen
                    </span>
                  )}
                </span>
              )}
            </div>

            <span className="rounded-full bg-[var(--surface-elevated)] px-2 py-0.5 text-xs font-semibold text-[var(--text-secondary)]">
              {filteredCount}
            </span>

            <div className="flex items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100">
              {isRenaming ? (
                <>
                  <button
                    type="button"
                    onClick={() => handleCommitRename(collection.id)}
                    className="rounded p-1 text-[var(--accent-primary)] hover:bg-[var(--surface-hover)]"
                    aria-label="Save rename"
                    title="Save"
                  >
                    <Check className="h-3.5 w-3.5" />
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      setRenamingCollectionId(null);
                      setRenameValue('');
                    }}
                    className="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
                    aria-label="Cancel rename"
                    title="Cancel"
                  >
                    <X className="h-3.5 w-3.5" />
                  </button>
                </>
              ) : (
                <>
                  <button
                    type="button"
                    onClick={() => handleCreateSubcollection(collection)}
                    className="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
                    aria-label="Create subfolder"
                    title="Create subfolder"
                    disabled={isSnapshot}
                  >
                    <FolderPlus className="h-3.5 w-3.5" />
                  </button>
                  <button
                    type="button"
                    onClick={() => handleStartRename(collection)}
                    className="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
                    aria-label="Rename collection"
                    title="Rename"
                  >
                    <Edit3 className="h-3.5 w-3.5" />
                  </button>
                  <button
                    type="button"
                    onClick={() => handleDeleteCustomCollection(collection.id)}
                    className="rounded p-1 text-[var(--error)] hover:bg-[var(--surface-hover)]"
                    aria-label="Delete collection"
                    title="Delete"
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </button>
                </>
              )}
            </div>
          </div>
        </div>

        {children.length > 0 && isExpanded && (
          <div className="space-y-1">
            {children.map(child => renderCustomCollectionNode(child, depth + 1))}
          </div>
        )}
      </div>
    );
  }, [
    customCollectionCountById,
    customCollectionsByParentId,
    collapsedCollectionIds,
    handleCommitRename,
    handleCreateSubcollection,
    handleDeleteCustomCollection,
    handleStartRename,
    renameValue,
    renamingCollectionId,
    selectedCollectionId,
    toggleCollectionCollapsed,
  ]);

  if (isLoading) {
    return <FileBrowserSkeleton view="tree" count={14} />;
  }

  if (error) {
    return (
      <div className="flex h-64 items-center justify-center">
        <div className="text-center text-[var(--error)]">
          <p className="mb-1 font-semibold">Error loading files</p>
          <p className="text-sm">{error}</p>
        </div>
      </div>
    );
  }

  if (allDocuments.length === 0) {
    return (
      <div className="flex h-64 items-center justify-center">
        <div className="text-center text-[var(--text-secondary)]">
          <p className="mb-1 font-semibold">No documents found</p>
          <p className="text-sm">Add files to start indexing</p>
        </div>
      </div>
    );
  }

  if (filteredDocuments.length === 0) {
    return (
      <div className="flex h-64 items-center justify-center">
        <div className="max-w-md px-6 text-center text-[var(--text-secondary)]">
          <p className="mb-1 font-semibold">No matches found</p>
          <p className="text-sm">
            {searchQuery.trim()
              ? `No files matched "${searchQuery.trim()}" by name or content.`
              : 'Try changing filters to see more files.'}
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="grid h-full min-h-0 grid-cols-1 lg:grid-cols-[340px_minmax(0,1fr)]">
      <aside className="flex min-h-0 flex-col border-b border-[var(--border-color)] bg-[linear-gradient(180deg,rgba(14,165,233,0.08),rgba(15,23,42,0.02))] lg:border-b-0 lg:border-r">
        <div className="border-b border-[var(--border-color)] bg-[var(--surface-elevated)]/80 px-4 py-3 backdrop-blur">
          <h3 className="text-xs font-semibold uppercase tracking-[0.16em] text-[var(--text-tertiary)]">
            Collections & Presets
          </h3>
          <p className="mt-1 text-sm text-[var(--text-secondary)]">
            Organize files with collections, then apply reusable presets.
          </p>

          <div className="mt-3 grid grid-cols-3 gap-2 rounded-xl border border-[var(--border-color)] bg-[var(--surface-elevated)] p-1">
            <button
              onClick={() => setLeftMode('collections')}
              className={`rounded-lg px-3 py-2 text-sm font-medium transition-colors ${
                leftMode === 'collections'
                  ? 'bg-[var(--accent-light)]/60 text-[var(--accent-primary)]'
                  : 'text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]'
              }`}
            >
              Collections
            </button>
            <button
              onClick={() => setLeftMode('presets')}
              className={`rounded-lg px-3 py-2 text-sm font-medium transition-colors ${
                leftMode === 'presets'
                  ? 'bg-[var(--accent-light)]/60 text-[var(--accent-primary)]'
                  : 'text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]'
              }`}
            >
              Presets
            </button>
            <button
              onClick={() => setLeftMode('sources')}
              className={`rounded-lg px-3 py-2 text-sm font-medium transition-colors ${
                leftMode === 'sources'
                  ? 'bg-[var(--accent-light)]/60 text-[var(--accent-primary)]'
                  : 'text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]'
              }`}
            >
              Sources
            </button>
          </div>
        </div>

        {leftMode === 'collections' ? (
          <div className="min-h-0 flex-1 overflow-auto p-3">
            <div className="space-y-3">
              <section className="rounded-2xl border border-[var(--border-color)] bg-[var(--surface-elevated)] p-3 shadow-[var(--shadow-sm)]">
                <h4 className="mb-3 px-1 text-xs font-semibold uppercase tracking-[0.14em] text-[var(--text-tertiary)]">
                  Switch Collections
                </h4>
                <div className="rounded-xl border border-[var(--border-color)] bg-[var(--bg-secondary)] p-2">
                    <div className="flex items-center gap-2">
                      <select
                        value={switcherSelectedCollectionId}
                        onChange={(event) => setSelectedCollectionId(event.target.value)}
                        className="h-9 w-full rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-sm font-medium text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                      >
                        <option value={COLLECTION_ALL}>
                          All Documents ({collections.find((collection) => collection.id === COLLECTION_ALL)?.docIds.size ?? filteredDocuments.length})
                        </option>
                        {projects.length > 0 && (
                          <optgroup label="Projects">
                            {projects.map((project) => (
                              <option key={project.id} value={project.id}>
                                {project.name} ({project.docIds.size})
                              </option>
                            ))}
                          </optgroup>
                        )}
                        {customCollectionSwitchOptions.length > 0 && (
                          <optgroup label="Custom Collections">
                            {customCollectionSwitchOptions.map((collection) => (
                              <option key={collection.id} value={collection.id}>
                                {collection.label} ({collection.count})
                              </option>
                            ))}
                          </optgroup>
                        )}
                      </select>
                      <button
                        type="button"
                        onClick={() => handleCycleCollection('prev')}
                        className="inline-flex h-9 w-9 items-center justify-center rounded-lg border border-[var(--border-color)] text-[var(--text-secondary)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
                        title="Previous collection"
                        aria-label="Previous collection"
                      >
                        <ArrowUp className="h-4 w-4" />
                      </button>
                      <button
                        type="button"
                        onClick={() => handleCycleCollection('next')}
                        className="inline-flex h-9 w-9 items-center justify-center rounded-lg border border-[var(--border-color)] text-[var(--text-secondary)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
                        title="Next collection"
                        aria-label="Next collection"
                      >
                        <ArrowDown className="h-4 w-4" />
                      </button>
                    </div>
                    <p className="mt-2 text-[11px] text-[var(--text-secondary)]">
                      {activeCollection?.description ?? 'Pick a collection to focus the document panel.'}
                    </p>
                </div>
              </section>

              <section className="rounded-2xl border border-[var(--border-color)] bg-[var(--surface-elevated)] p-3 shadow-[var(--shadow-sm)]">
                <h4 className="mb-3 px-1 text-xs font-semibold uppercase tracking-[0.14em] text-[var(--text-tertiary)]">
                  Custom Collections
                </h4>
                <div className="rounded-xl border border-[var(--border-color)] bg-[var(--bg-secondary)] p-2">
                  <div className="mb-2 flex items-center justify-between">
                    <span className="text-xs font-semibold uppercase tracking-wide text-[var(--text-tertiary)]">
                      Browse
                    </span>
                    <span className="rounded-full border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-[var(--text-secondary)]">
                      {customCollections.length}
                    </span>
                  </div>
                  {customCollections.length === 0 ? (
                    <p className="px-1 py-2 text-xs text-[var(--text-secondary)]">No collections yet.</p>
                  ) : (
                    <div className="space-y-1">
                      {(customCollectionsByParentId.get(null) ?? []).map((collection) =>
                        renderCustomCollectionNode(collection, 0)
                      )}
                    </div>
                  )}
                </div>
              </section>

              <section className="rounded-2xl border border-[var(--border-color)] bg-[var(--surface-elevated)] p-3 shadow-[var(--shadow-sm)]">
                <h4 className="mb-3 px-1 text-xs font-semibold uppercase tracking-[0.14em] text-[var(--text-tertiary)]">
                  Collection Builder
                </h4>
                <div className="rounded-xl border border-[var(--border-color)] bg-[var(--bg-secondary)] p-2">
                    <div className="flex items-center gap-2">
                      <input
                        type="text"
                        value={newCollectionName}
                        onChange={(event) => setNewCollectionName(event.target.value)}
                        placeholder="New collection name..."
                        className="h-8 w-full rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                      />
                      <button
                        type="button"
                        onClick={() =>
                          handleCreateCustomCollection(
                            activeCustomCollection?.kind === 'manual' ? activeCustomCollection.id : null
                          )
                        }
                        className="inline-flex h-8 items-center justify-center rounded-lg border border-[var(--border-color)] px-2 text-xs font-semibold text-[var(--text-secondary)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
                        aria-label="Create collection"
                        title="Create collection"
                      >
                        <Plus className="mr-1 h-3.5 w-3.5" />
                        Create
                      </button>
                      <button
                        type="button"
                        onClick={() =>
                          handleCreateSnapshotFromSelection(
                            activeCustomCollection?.kind === 'manual' ? activeCustomCollection.id : null
                          )
                        }
                        className="inline-flex h-8 items-center justify-center rounded-lg border border-[var(--border-color)] px-2 text-xs font-semibold text-[var(--text-secondary)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
                        aria-label="Freeze selected files into a collection"
                        title="Freeze selected files into a collection"
                      >
                        <Copy className="mr-1 h-3.5 w-3.5" />
                        Freeze
                      </button>
                    </div>
                    <p className="mt-2 text-[11px] text-[var(--text-secondary)]">
                      Create manual folders. Use Freeze to capture selected files as a locked collection.
                    </p>

                  {activeCustomCollection && (
                    <div className="mt-3 rounded-xl border border-[var(--border-color)] bg-[var(--surface-elevated)]/70 p-2">
                      <div className="mb-2 flex items-center justify-between gap-2">
                        <span className="text-xs font-semibold uppercase tracking-wide text-[var(--text-tertiary)]">
                          Edit Collection
                        </span>
                        <span className="rounded-full border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-[var(--text-secondary)]">
                          {activeCustomCollection.kind === 'snapshot' ? 'frozen' : 'manual'}
                        </span>
                      </div>

                      <div className="space-y-2">
                        <input
                          type="text"
                          value={collectionEditorName}
                          onChange={(event) => setCollectionEditorName(event.target.value)}
                          className="h-8 w-full rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                          placeholder="Collection name"
                        />
                        <select
                          value={collectionEditorParentId ?? ''}
                          onChange={(event) => setCollectionEditorParentId(event.target.value || null)}
                          className="h-8 w-full rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                        >
                          <option value="">No parent (top level)</option>
                          {collectionEditorParentOptions.map((collection) => (
                            <option key={collection.id} value={collection.id}>
                              {collection.name}
                            </option>
                          ))}
                        </select>
                      </div>

                      <div className="mt-2 grid grid-cols-2 gap-2">
                        <button
                          type="button"
                          onClick={handleSaveCollectionEditor}
                          className="inline-flex items-center justify-center rounded-lg border border-[var(--accent-primary)]/35 bg-[var(--accent-light)]/45 px-2 py-1.5 text-xs font-semibold text-[var(--accent-primary)] transition-colors hover:bg-[var(--accent-light)]/65"
                        >
                          <Check className="mr-1 h-3.5 w-3.5" />
                          Save
                        </button>
                        <button
                          type="button"
                          onClick={() => handleDeleteCustomCollection(activeCustomCollection.id)}
                          className="inline-flex items-center justify-center rounded-lg border border-[var(--error)]/35 px-2 py-1.5 text-xs font-semibold text-[var(--error)] transition-colors hover:bg-[var(--error)]/10"
                        >
                          <Trash2 className="mr-1 h-3.5 w-3.5" />
                          Delete
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              </section>
            </div>
          </div>
        ) : leftMode === 'presets' ? (
          <div className="min-h-0 flex-1 overflow-auto p-3">
            <section className="rounded-2xl border border-[var(--border-color)] bg-[var(--surface-elevated)] p-3 shadow-[var(--shadow-sm)]">
              <div className="rounded-xl border border-[var(--border-color)] bg-[var(--bg-secondary)] px-3 py-2">
                <p className="text-xs font-semibold uppercase tracking-[0.12em] text-[var(--text-tertiary)]">
                  Preset Guide
                </p>
                <p className="mt-1 text-xs text-[var(--text-secondary)]">
                  Search presets save what to find. View presets save how results are displayed.
                </p>
              </div>
            </section>

            <section className="mt-3 rounded-2xl border border-[var(--border-color)] bg-[var(--surface-elevated)] p-3 shadow-[var(--shadow-sm)]">
              <h4 className="mb-3 px-1 text-xs font-semibold uppercase tracking-[0.14em] text-[var(--text-tertiary)]">
                Search Presets
              </h4>
              <div className="rounded-xl border border-[var(--border-color)] bg-[var(--bg-secondary)] p-2">
                <div className="flex items-center gap-2">
                  <input
                    type="text"
                    value={newSearchName}
                    onChange={(event) => setNewSearchName(event.target.value)}
                    placeholder="New search name..."
                    className="h-8 w-full rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                  />
                  <button
                    type="button"
                    onClick={handleCreateSavedSearch}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-lg border border-[var(--border-color)] text-[var(--text-secondary)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
                    aria-label="Create saved search"
                    title="Save current search + source/type filters"
                  >
                    <Plus className="h-4 w-4" />
                  </button>
                </div>

                {savedSearches.length === 0 ? (
                  <p className="px-1 py-2 text-xs text-[var(--text-secondary)]">
                    No saved searches yet.
                  </p>
                ) : (
                  <div className="mt-2 space-y-1">
                    {savedSearches.map((search) => {
                      const isActive = activeSavedSearchId === search.id;
                      const isRenaming = renamingSearchId === search.id;
                      const searchIndex = savedSearches.findIndex((candidate) => candidate.id === search.id);
                      const isFirst = searchIndex === 0;
                      const isLast = searchIndex === savedSearches.length - 1;
                      return (
                        <div
                          key={search.id}
                          className={`rounded-lg border px-2 py-1.5 ${
                            isActive
                              ? 'border-[var(--accent-primary)]/35 bg-[var(--accent-light)]/35'
                              : 'border-[var(--border-color)] bg-[var(--bg-secondary)]'
                          }`}
                        >
                          <div className="flex items-center justify-between gap-2">
                            {isRenaming ? (
                              <input
                                type="text"
                                value={renameSearchValue}
                                onChange={(event) => setRenameSearchValue(event.target.value)}
                                className="h-7 min-w-0 flex-1 rounded border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                                onClick={(event) => event.stopPropagation()}
                              />
                            ) : (
                              <button
                                type="button"
                                onClick={() => applySavedSearch(search.id)}
                                className="min-w-0 flex-1 truncate text-left text-sm font-medium text-[var(--text-primary)]"
                              >
                                {search.name}
                              </button>
                            )}
                            <div className="flex items-center gap-1">
                              {isRenaming ? (
                                <>
                                  <button
                                    type="button"
                                    onClick={() => handleCommitRenameSavedSearch(search.id)}
                                    className="rounded p-1 text-[var(--accent-primary)] hover:bg-[var(--surface-hover)]"
                                    aria-label="Save search rename"
                                    title="Save"
                                  >
                                    <Check className="h-3.5 w-3.5" />
                                  </button>
                                  <button
                                    type="button"
                                    onClick={() => {
                                      setRenamingSearchId(null);
                                      setRenameSearchValue('');
                                    }}
                                    className="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
                                    aria-label="Cancel search rename"
                                    title="Cancel"
                                  >
                                    <X className="h-3.5 w-3.5" />
                                  </button>
                                </>
                              ) : (
                                <>
                                  <button
                                    type="button"
                                    onClick={() => handleMoveSavedSearch(search.id, 'up')}
                                    className="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)] disabled:cursor-not-allowed disabled:opacity-40"
                                    aria-label="Move search up"
                                    title="Move up"
                                    disabled={isFirst}
                                  >
                                    <ArrowUp className="h-3.5 w-3.5" />
                                  </button>
                                  <button
                                    type="button"
                                    onClick={() => handleMoveSavedSearch(search.id, 'down')}
                                    className="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)] disabled:cursor-not-allowed disabled:opacity-40"
                                    aria-label="Move search down"
                                    title="Move down"
                                    disabled={isLast}
                                  >
                                    <ArrowDown className="h-3.5 w-3.5" />
                                  </button>
                                  <button
                                    type="button"
                                    onClick={() => handleStartRenameSavedSearch(search.id, search.name)}
                                    className="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
                                    aria-label="Rename saved search"
                                    title="Rename saved search"
                                  >
                                    <Edit3 className="h-3.5 w-3.5" />
                                  </button>
                                  <button
                                    type="button"
                                    onClick={() => handleDuplicateSavedSearch(search.id)}
                                    className="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
                                    aria-label="Duplicate saved search"
                                    title="Duplicate saved search"
                                  >
                                    <Copy className="h-3.5 w-3.5" />
                                  </button>
                                </>
                              )}
                              <button
                                type="button"
                                onClick={() => handleTogglePinnedSearch(search.id, search.pinned)}
                                className="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
                                aria-label={search.pinned ? 'Unpin search' : 'Pin search'}
                                title={search.pinned ? 'Unpin search' : 'Pin search'}
                              >
                                {search.pinned ? (
                                  <PinOff className="h-3.5 w-3.5" />
                                ) : (
                                  <Pin className="h-3.5 w-3.5" />
                                )}
                              </button>
                              <button
                                type="button"
                                onClick={() => deleteSavedSearch(search.id)}
                                className="rounded p-1 text-[var(--error)] hover:bg-[var(--surface-hover)]"
                                aria-label="Delete saved search"
                                title="Delete saved search"
                              >
                                <Trash2 className="h-3.5 w-3.5" />
                              </button>
                            </div>
                          </div>
                          <p className="mt-1 truncate text-[11px] text-[var(--text-secondary)]">
                            {search.query.trim() ? search.query : 'No keyword query'} • {search.filterBySource} source
                            {search.filterByType ? ` • type: ${search.filterByType}` : ''}
                          </p>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            </section>

            <section className="mt-3 rounded-2xl border border-[var(--border-color)] bg-[var(--surface-elevated)] p-3 shadow-[var(--shadow-sm)]">
              <h4 className="mb-3 px-1 text-xs font-semibold uppercase tracking-[0.14em] text-[var(--text-tertiary)]">
                View Presets
              </h4>
              <div className="rounded-xl border border-[var(--border-color)] bg-[var(--bg-secondary)] p-2">
                <div className="flex items-center gap-2">
                  <input
                    type="text"
                    value={newViewName}
                    onChange={(event) => setNewViewName(event.target.value)}
                    placeholder="New view name..."
                    className="h-8 w-full rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-sm text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                  />
                  <button
                    type="button"
                    onClick={handleCreateSavedView}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-lg border border-[var(--border-color)] text-[var(--text-secondary)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
                    aria-label="Create view"
                    title="Create view from current controls"
                  >
                    <Plus className="h-4 w-4" />
                  </button>
                </div>

                {savedViews.length === 0 ? (
                  <p className="px-1 py-2 text-xs text-[var(--text-secondary)]">
                    No view presets yet. Save one from your current filters and sort settings.
                  </p>
                ) : (
                  <div className="mt-2 space-y-1">
                    {savedViews.map((view) => {
                      const isSelected = selectedViewId === view.id;
                      const isApplied = activeSavedViewId === view.id;

                      return (
                        <div
                          key={view.id}
                          className={`rounded-lg border px-2 py-1.5 ${
                            isSelected
                              ? 'border-[var(--accent-primary)]/35 bg-[var(--accent-light)]/35'
                              : 'border-[var(--border-color)] bg-[var(--bg-secondary)]'
                          }`}
                        >
                          <div className="flex items-center justify-between gap-2">
                            <button
                              type="button"
                              onClick={() => setSelectedViewId(view.id)}
                              className="flex min-w-0 flex-1 items-center gap-2 text-left"
                            >
                              <span className="truncate text-sm font-medium text-[var(--text-primary)]">
                                {view.name}
                              </span>
                              {isApplied && (
                                <span className="rounded-full bg-[var(--accent-light)]/60 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-[var(--accent-primary)]">
                                  Active
                                </span>
                              )}
                            </button>

                            <div className="flex items-center gap-1">
                              <button
                                type="button"
                                onClick={() => handleApplySavedView(view.id)}
                                className="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
                                title="Apply view"
                                aria-label="Apply view"
                              >
                                <Check className="h-3.5 w-3.5" />
                              </button>
                              <button
                                type="button"
                                onClick={() => handleCaptureSavedView(view.id)}
                                className="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
                                title="Refresh from current controls"
                                aria-label="Refresh from current controls"
                              >
                                <RefreshCw className="h-3.5 w-3.5" />
                              </button>
                              <button
                                type="button"
                                onClick={() => handleDeleteSavedView(view.id)}
                                className="rounded p-1 text-[var(--error)] hover:bg-[var(--surface-hover)]"
                                title="Delete view"
                                aria-label="Delete view"
                              >
                                <Trash2 className="h-3.5 w-3.5" />
                              </button>
                            </div>
                          </div>
                          <p className="mt-1 text-[11px] text-[var(--text-secondary)]">
                            {view.viewMode} • {view.sortField} ({view.sortOrder}) • {view.filterBySource} source
                            {view.baseCollectionId &&
                              ` • scoped: ${customCollectionsById.get(view.baseCollectionId)?.name ?? 'collection'}`}
                          </p>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            </section>

            <section className="mt-3 rounded-2xl border border-[var(--border-color)] bg-[var(--surface-elevated)] p-3 shadow-[var(--shadow-sm)]">
              <h4 className="mb-3 px-1 text-xs font-semibold uppercase tracking-[0.14em] text-[var(--text-tertiary)]">
                Preset Editor
              </h4>
              <div className="space-y-2 rounded-xl border border-[var(--border-color)] bg-[var(--bg-secondary)] p-3">
                {!selectedSavedView || !viewDraft ? (
                  <p className="text-xs text-[var(--text-secondary)]">
                    Select a view preset to customize it.
                  </p>
                ) : (
                  <>
                    <input
                      type="text"
                      value={viewDraft.name}
                      onChange={(event) =>
                        setViewDraft((current) => (current ? { ...current, name: event.target.value } : current))
                      }
                      placeholder="View name"
                      className="h-8 w-full rounded-md border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-xs text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                    />
                    <select
                      value={viewDraft.baseCollectionId ?? ''}
                      onChange={(event) =>
                        setViewDraft((current) =>
                          current
                            ? {
                                ...current,
                                baseCollectionId: event.target.value || null,
                              }
                            : current
                        )
                      }
                      className="h-8 rounded-md border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-xs text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                    >
                      <option value="">Scope: All documents</option>
                      {customCollections.map((collection) => (
                        <option key={collection.id} value={collection.id}>
                          Scope: {collection.name}
                        </option>
                      ))}
                    </select>
                    <div className="grid grid-cols-2 gap-2">
                      <select
                        value={viewDraft.viewMode}
                        onChange={(event) =>
                          setViewDraft((current) =>
                            current ? { ...current, viewMode: event.target.value as ViewMode } : current
                          )
                        }
                        className="h-8 rounded-md border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-xs text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                      >
                        <option value="tree">Tree</option>
                        <option value="list">List</option>
                        <option value="grid">Grid</option>
                      </select>
                      <select
                        value={viewDraft.density}
                        onChange={(event) =>
                          setViewDraft((current) =>
                            current ? { ...current, density: event.target.value as Density } : current
                          )
                        }
                        className="h-8 rounded-md border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-xs text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                      >
                        <option value="compact">Compact</option>
                        <option value="comfortable">Comfortable</option>
                        <option value="spacious">Spacious</option>
                      </select>
                    </div>
                    <div className="grid grid-cols-2 gap-2">
                      <select
                        value={viewDraft.sortField}
                        onChange={(event) =>
                          setViewDraft((current) =>
                            current ? { ...current, sortField: event.target.value as SortField } : current
                          )
                        }
                        className="h-8 rounded-md border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-xs text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                      >
                        <option value="name">Sort: Name</option>
                        <option value="modified">Sort: Modified</option>
                        <option value="size">Sort: Size</option>
                        <option value="type">Sort: Type</option>
                      </select>
                      <select
                        value={viewDraft.sortOrder}
                        onChange={(event) =>
                          setViewDraft((current) =>
                            current ? { ...current, sortOrder: event.target.value as SortOrder } : current
                          )
                        }
                        className="h-8 rounded-md border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-xs text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                      >
                        <option value="asc">Asc</option>
                        <option value="desc">Desc</option>
                      </select>
                    </div>
                    <div className="grid grid-cols-2 gap-2">
                      <select
                        value={viewDraft.filterBySource}
                        onChange={(event) =>
                          setViewDraft((current) =>
                            current ? { ...current, filterBySource: event.target.value as SourceFilter } : current
                          )
                        }
                        className="h-8 rounded-md border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-xs text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                      >
                        <option value="all">All sources</option>
                        <option value="local">Local only</option>
                        <option value="web">Web only</option>
                      </select>
                      <input
                        type="text"
                        value={viewDraft.filterByType}
                        onChange={(event) =>
                          setViewDraft((current) =>
                            current ? { ...current, filterByType: event.target.value } : current
                          )
                        }
                        placeholder="Type filter (optional)"
                        className="h-8 rounded-md border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-xs text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                      />
                    </div>
                    <input
                      type="text"
                      value={viewDraft.searchQuery}
                      onChange={(event) =>
                        setViewDraft((current) =>
                          current ? { ...current, searchQuery: event.target.value } : current
                        )
                      }
                      placeholder="Saved search query (optional)"
                      className="h-8 w-full rounded-md border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-xs text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                    />
                    {viewDraft.viewMode === 'list' && (
                      <div className="rounded-md border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 py-2">
                        <p className="mb-1 text-[11px] font-semibold uppercase tracking-wide text-[var(--text-tertiary)]">
                          List Columns
                        </p>
                        <div className="flex flex-wrap gap-3 text-xs text-[var(--text-secondary)]">
                          <label className="inline-flex items-center gap-1.5">
                            <input
                              type="checkbox"
                              checked={viewDraft.listColumns.words}
                              onChange={(event) =>
                                setViewDraft((current) =>
                                  current
                                    ? {
                                        ...current,
                                        listColumns: {
                                          ...current.listColumns,
                                          words: event.target.checked,
                                        },
                                      }
                                    : current
                                )
                              }
                              className="h-3.5 w-3.5 rounded border-[var(--border-color)] bg-[var(--bg-secondary)]"
                            />
                            Words
                          </label>
                          <label className="inline-flex items-center gap-1.5">
                            <input
                              type="checkbox"
                              checked={viewDraft.listColumns.modified}
                              onChange={(event) =>
                                setViewDraft((current) =>
                                  current
                                    ? {
                                        ...current,
                                        listColumns: {
                                          ...current.listColumns,
                                          modified: event.target.checked,
                                        },
                                      }
                                    : current
                                )
                              }
                              className="h-3.5 w-3.5 rounded border-[var(--border-color)] bg-[var(--bg-secondary)]"
                            />
                            Modified
                          </label>
                          <label className="inline-flex items-center gap-1.5">
                            <input
                              type="checkbox"
                              checked={viewDraft.listColumns.type}
                              onChange={(event) =>
                                setViewDraft((current) =>
                                  current
                                    ? {
                                        ...current,
                                        listColumns: {
                                          ...current.listColumns,
                                          type: event.target.checked,
                                        },
                                      }
                                    : current
                                )
                              }
                              className="h-3.5 w-3.5 rounded border-[var(--border-color)] bg-[var(--bg-secondary)]"
                            />
                            Type
                          </label>
                        </div>
                      </div>
                    )}
                    <label className="inline-flex items-center gap-2 text-xs text-[var(--text-secondary)]">
                      <input
                        type="checkbox"
                        checked={viewDraft.groupByDate}
                        onChange={(event) =>
                          setViewDraft((current) =>
                            current ? { ...current, groupByDate: event.target.checked } : current
                          )
                        }
                        className="h-3.5 w-3.5 rounded border-[var(--border-color)] bg-[var(--bg-secondary)]"
                      />
                      Group by date
                    </label>
                    <div className="grid grid-cols-2 gap-2">
                      <button
                        type="button"
                        onClick={handleLoadCurrentControlsIntoDraft}
                        className="inline-flex h-8 items-center justify-center rounded-md border border-[var(--border-color)] text-xs font-semibold text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
                      >
                        Use Current Controls
                      </button>
                      <button
                        type="button"
                        onClick={handleSaveViewDraft}
                        className="inline-flex h-8 items-center justify-center rounded-md border border-[var(--border-color)] text-xs font-semibold text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
                      >
                        Save Changes
                      </button>
                    </div>
                  </>
                )}
              </div>
            </section>
          </div>
        ) : (
          <div className="min-h-0 flex-1 overflow-auto p-3">
            <div className="grid grid-cols-2 gap-2 rounded-2xl border border-[var(--border-color)] bg-[var(--surface-elevated)] p-3 shadow-[var(--shadow-sm)]">
              <SourceCard
                title="Local"
                count={sources.local}
                icon={<HardDrive className="h-4 w-4" />}
                onClick={() => setSelectedCollectionId(COLLECTION_LOCAL)}
              />
              <SourceCard
                title="Web"
                count={sources.web}
                icon={<Globe2 className="h-4 w-4" />}
                onClick={() => setSelectedCollectionId(COLLECTION_WEB)}
              />
            </div>

            <div className="mt-3 rounded-2xl border border-[var(--border-color)] bg-[var(--surface-elevated)] p-3 shadow-[var(--shadow-sm)]">
              <div className="rounded-xl border border-[var(--border-color)] bg-[var(--bg-secondary)] p-3">
                <p className="text-sm font-medium text-[var(--text-primary)]">Cloud Sync Ready</p>
                <p className="mt-1 text-xs text-[var(--text-secondary)]">
                  Sources are separated from collections so cloud providers can plug into the same model.
                </p>
                <button
                  type="button"
                  className="mt-3 inline-flex items-center gap-1 text-xs font-semibold text-[var(--accent-primary)]"
                >
                  Explore sync architecture
                  <ExternalLink className="h-3.5 w-3.5" />
                </button>
              </div>
            </div>

            <div className="mt-3 rounded-2xl border border-[var(--border-color)] bg-[var(--surface-elevated)] p-3 shadow-[var(--shadow-sm)]">
              <div className="mb-2 flex items-center justify-between">
                <h4 className="text-xs font-semibold uppercase tracking-[0.14em] text-[var(--text-tertiary)]">
                  Source Connections
                </h4>
                <button
                  type="button"
                  onClick={() => {
                    void syncLocalSources();
                  }}
                  className="inline-flex items-center gap-1 rounded-md border border-[var(--border-color)] px-2 py-1 text-[11px] font-medium text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
                >
                  {isSyncingSources ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <RefreshCw className="h-3.5 w-3.5" />}
                  Sync
                </button>
              </div>

              <div className="mb-2 space-y-2 rounded-xl border border-[var(--border-color)] bg-[var(--bg-secondary)] p-2">
                <p className="text-[11px] font-semibold uppercase tracking-wide text-[var(--text-tertiary)]">
                  Add Cloud Source
                </p>
                <input
                  type="text"
                  value={newSourceName}
                  onChange={(event) => setNewSourceName(event.target.value)}
                  placeholder="e.g. Product Docs Drive"
                  className="h-8 w-full rounded-md border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-xs text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                />
                <div className="grid grid-cols-2 gap-2">
                  <select
                    value={newSourceProvider}
                    onChange={(event) => setNewSourceProvider(event.target.value as SourceProvider)}
                    className="h-8 rounded-md border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-xs text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                  >
                    <option value="dropbox">Dropbox</option>
                    <option value="google_drive">Google Drive</option>
                    <option value="onedrive">OneDrive</option>
                    <option value="icloud">iCloud</option>
                    <option value="web_import">Web Feed</option>
                  </select>
                  <select
                    value={newSourceMode}
                    onChange={(event) => setNewSourceMode(event.target.value as SourceConnection['mode'])}
                    className="h-8 rounded-md border border-[var(--border-color)] bg-[var(--bg-secondary)] px-2 text-xs text-[var(--text-primary)] outline-none focus:border-[var(--accent-primary)]"
                  >
                    <option value="referenced">Referenced</option>
                    <option value="managed">Managed</option>
                  </select>
                </div>
                <button
                  type="button"
                  onClick={handleCreateCloudSource}
                  className="inline-flex h-8 w-full items-center justify-center gap-1 rounded-md border border-[var(--border-color)] text-xs font-semibold text-[var(--text-secondary)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
                >
                  <Plus className="h-3.5 w-3.5" />
                  Add Source
                </button>
              </div>

              {sourceConnections.length === 0 ? (
                <div className="rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)] px-3 py-2 text-xs text-[var(--text-secondary)]">
                  No source connections yet.
                </div>
              ) : (
                <div className="space-y-1.5">
                  {sourceConnections.map((source) => (
                    <SourceConnectionCard
                      key={source.id}
                      source={source}
                      indexedCount={sourceCollectionDefs.find(def => def.id === source.id)?.docIds.size ?? 0}
                      isActive={selectedCollectionId === source.id}
                      onFocus={() => setSelectedCollectionId(source.id)}
                      onToggleMode={(mode) => updateSourceConnection(source.id, { mode })}
                      onToggleEnabled={(enabled) =>
                        updateSourceConnection(source.id, { enabled, health: enabled ? 'healthy' : 'paused' })
                      }
                      onRemove={() => deleteSourceConnection(source.id)}
                    />
                  ))}
                </div>
              )}
            </div>

            <div className="mt-3 rounded-2xl border border-[var(--border-color)] bg-[var(--surface-elevated)] p-2 shadow-[var(--shadow-sm)]">
              <FolderList className="min-h-[320px]" />
            </div>
          </div>
        )}
      </aside>

      <section className="flex min-h-0 flex-col bg-[linear-gradient(180deg,var(--surface-elevated),var(--bg-secondary))]">
        <header className="border-b border-[var(--border-color)] bg-[var(--surface-elevated)]/85 px-5 py-3 backdrop-blur">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <p className="text-xs font-semibold uppercase tracking-[0.14em] text-[var(--text-tertiary)]">
                Collection Focus
              </p>
              <h3 className="text-lg font-semibold text-[var(--text-primary)]">
                {activeCollection?.name ?? 'Library'}
              </h3>
              <p className="text-xs text-[var(--text-secondary)]">
                {activeCollection?.description ?? 'Organized document view'}
              </p>
            </div>

            <div className="flex items-center gap-3">
              <span className="rounded-full border border-[var(--border-color)] bg-[var(--surface-elevated)] px-3 py-1.5 text-xs font-semibold text-[var(--text-secondary)]">
                {visibleDocuments.length} file{visibleDocuments.length === 1 ? '' : 's'}
              </span>
              {activeCustomCollection && (
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={handleAddSelectedToActiveCollection}
                    disabled={selectedIds.length === 0 || activeCustomCollection.kind === 'snapshot'}
                    className="rounded-lg border border-[var(--border-color)] px-2.5 py-1.5 text-xs font-semibold text-[var(--text-secondary)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)] disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    Add selected
                  </button>
                  <button
                    type="button"
                    onClick={handleRemoveSelectedFromActiveCollection}
                    disabled={selectedIdsInActiveCustomCollection.length === 0 || activeCustomCollection.kind === 'snapshot'}
                    className="rounded-lg border border-[var(--border-color)] px-2.5 py-1.5 text-xs font-semibold text-[var(--text-secondary)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)] disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    Remove selected
                  </button>
                  {activeCustomCollection.kind === 'snapshot' && (
                    <span className="rounded-full border border-[var(--border-color)] bg-[var(--surface-elevated)] px-2 py-1 text-[10px] font-semibold uppercase tracking-wide text-[var(--text-tertiary)]">
                      Frozen
                    </span>
                  )}
                </div>
              )}
              <Checkbox
                checked={allVisibleSelected}
                indeterminate={someVisibleSelected}
                onChange={toggleSelectAllVisible}
                aria-label="Select all visible files"
              />
            </div>
          </div>
        </header>

        <div ref={documentsPanelRef} className="flex-1 overflow-auto p-3">
          {visibleDocuments.length === 0 ? (
            <div className="flex h-full items-center justify-center rounded-2xl border border-[var(--border-color)] bg-[var(--surface-elevated)] text-sm text-[var(--text-secondary)]">
              No files in this collection.
            </div>
          ) : (
            <div className="rounded-2xl border border-[var(--border-color)] bg-[var(--surface-elevated)] p-1 shadow-[var(--shadow-sm)]">
              <div
                style={{
                  height: `${documentVirtualizer.getTotalSize()}px`,
                  position: 'relative',
                }}
              >
                {documentVirtualizer.getVirtualItems().map((virtualRow) => {
                  const doc = visibleDocuments[virtualRow.index];
                  const isSelected = selectedDocumentIds.has(doc.id);

                  return (
                    <div
                      key={doc.id}
                      style={{
                        position: 'absolute',
                        top: 0,
                        left: 0,
                        width: '100%',
                        height: `${virtualRow.size}px`,
                        transform: `translateY(${virtualRow.start}px)`,
                      }}
                      className="px-2"
                    >
                      <div
                        className={`group flex h-full items-center gap-3 rounded-xl border px-3 py-2 transition-all ${
                          isSelected
                            ? 'border-[var(--accent-primary)]/35 bg-[var(--accent-light)]/30'
                            : 'border-transparent hover:border-[var(--border-color)] hover:bg-[var(--surface-hover)]/70'
                        }`}
                        onClick={(event) => handleDocumentClick(doc, virtualRow.index, event)}
                        onDoubleClick={() => onFileOpen?.(doc)}
                        onContextMenu={(event) => {
                          event.preventDefault();
                          onContextMenu?.(event, doc);
                        }}
                      >
                        <div onClick={(event) => event.stopPropagation()}>
                          <Checkbox
                            checked={isSelected}
                            onChange={() => toggleSelection(doc.id)}
                            aria-label={`Select ${doc.fileName}`}
                          />
                        </div>

                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-2">
                            <FileIcon file={doc} size={18} />
                            <span className="truncate text-sm font-medium text-[var(--text-primary)]">{doc.fileName}</span>
                            {isWebDocument(doc) ? (
                              <Globe2 className="h-3.5 w-3.5 shrink-0 text-[var(--accent-primary)]" />
                            ) : (
                              <Link className="h-3.5 w-3.5 shrink-0 text-[var(--text-tertiary)]" />
                            )}
                          </div>
                          <div className="mt-0.5 flex items-center gap-2 text-xs text-[var(--text-secondary)]">
                            <span>{doc.wordCount ? `${doc.wordCount} words` : '-'}</span>
                            <span>•</span>
                            <span>{formatRelativeTime(doc.modifiedAt)}</span>
                          </div>
                        </div>

                        <div className="hidden items-center gap-2 md:flex">
                          <FileTypeBadge file={doc} size="sm" />

                          <div className="flex items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100">
                            {onRename && (
                              <button
                                onClick={(event) => {
                                  event.stopPropagation();
                                  onRename(doc);
                                }}
                                className="rounded-lg p-1.5 text-[var(--text-secondary)] hover:bg-[var(--surface-elevated)] hover:text-[var(--text-primary)]"
                                aria-label="Rename"
                                title="Rename"
                              >
                                <Edit3 className="h-4 w-4" />
                              </button>
                            )}

                            {onDelete && (
                              <button
                                onClick={(event) => {
                                  event.stopPropagation();
                                  onDelete(doc);
                                }}
                                className="rounded-lg p-1.5 text-[var(--error)] hover:bg-[var(--surface-elevated)]"
                                aria-label="Delete"
                                title="Delete"
                              >
                                <Trash2 className="h-4 w-4" />
                              </button>
                            )}
                          </div>
                        </div>
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </div>
      </section>
    </div>
  );
};

interface SourceCardProps {
  title: string;
  count: number;
  icon: React.ReactNode;
  onClick?: () => void;
}

function SourceCard({ title, count, icon, onClick }: SourceCardProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="rounded-xl border border-[var(--border-color)] bg-[var(--surface-elevated)] px-3 py-2 text-left transition-all hover:-translate-y-0.5 hover:border-[var(--accent-primary)]/30 hover:shadow-[var(--shadow-md)]"
    >
      <span className="inline-flex items-center gap-1 text-xs font-semibold uppercase tracking-wide text-[var(--text-tertiary)]">
        {icon}
        {title}
      </span>
      <p className="mt-1 text-lg font-semibold text-[var(--text-primary)]">{count}</p>
      <p className="text-xs text-[var(--text-secondary)]">indexed items</p>
    </button>
  );
}

interface SourceConnectionCardProps {
  source: SourceConnection;
  indexedCount: number;
  isActive: boolean;
  onFocus: () => void;
  onToggleMode: (mode: SourceConnection['mode']) => void;
  onToggleEnabled: (enabled: boolean) => void;
  onRemove: () => void;
}

function SourceConnectionCard({
  source,
  indexedCount,
  isActive,
  onFocus,
  onToggleMode,
  onToggleEnabled,
  onRemove,
}: SourceConnectionCardProps) {
  const providerLabel = source.provider
    .replace(/_/g, ' ')
    .replace(/\b\w/g, letter => letter.toUpperCase());

  return (
    <div
      className={`rounded-xl border bg-[var(--surface-elevated)] px-3 py-2 transition-colors ${
        isActive
          ? 'border-[var(--accent-primary)]/40'
          : 'border-[var(--border-color)]'
      }`}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <button
            type="button"
            onClick={onFocus}
            className="flex items-center gap-2 text-left"
          >
            <p className="truncate text-sm font-medium text-[var(--text-primary)]">{source.name}</p>
            <span className="rounded-full border border-[var(--border-color)] px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-[var(--text-tertiary)]">
              {providerLabel}
            </span>
          </button>
          <p className="truncate text-[11px] text-[var(--text-tertiary)]">
            {source.path ?? source.provider} • {indexedCount} indexed
          </p>
        </div>
        <button
          type="button"
          onClick={onRemove}
          className="rounded p-1 text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--error)]"
          title="Remove source entry"
          aria-label="Remove source entry"
        >
          <Trash2 className="h-3.5 w-3.5" />
        </button>
      </div>

      <div className="mt-2 flex items-center gap-2">
        <button
          type="button"
          onClick={() => onToggleMode('referenced')}
          className={`rounded-md px-2 py-1 text-[11px] font-semibold ${
            source.mode === 'referenced'
              ? 'bg-[var(--accent-light)]/60 text-[var(--accent-primary)]'
              : 'bg-[var(--bg-secondary)] text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]'
          }`}
        >
          Referenced
        </button>
        <button
          type="button"
          onClick={() => onToggleMode('managed')}
          className={`rounded-md px-2 py-1 text-[11px] font-semibold ${
            source.mode === 'managed'
              ? 'bg-[var(--accent-light)]/60 text-[var(--accent-primary)]'
              : 'bg-[var(--bg-secondary)] text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]'
          }`}
        >
          Managed
        </button>

        <button
          type="button"
          onClick={() => onToggleEnabled(!source.enabled)}
          className={`ml-auto rounded-md px-2 py-1 text-[11px] font-semibold ${
            source.enabled
              ? 'bg-[var(--success)]/15 text-[var(--success)]'
              : 'bg-[var(--bg-secondary)] text-[var(--text-secondary)]'
          }`}
        >
          {source.enabled ? 'Enabled' : 'Paused'}
        </button>
      </div>

      <p className="mt-1 text-[11px] text-[var(--text-tertiary)]">
        Last sync: {source.lastSyncedAt ? formatRelativeTime(source.lastSyncedAt) : 'Never'} • Health: {source.health}
      </p>
    </div>
  );
}
