/**
 * Library — the corpus surface.
 *
 * Header (title + one data line + view toggles), one toolbar row, an optional
 * rail of the things the corpus is organised by, and the documents themselves
 * as hairline rows. No cards, no badges, no explanatory copy.
 */

import { useState, useCallback, useEffect, useMemo, useRef } from 'react';

import { open as openExternal } from '@tauri-apps/plugin-shell';
import { EyeOff, LayoutGrid, Layers, List, ListTree, MessageSquare, PanelRight, RefreshCw } from 'lucide-react';
import { useNavigate } from 'react-router';

import { ConfirmDialog } from '@/components/ConfirmDialog';
import { EmptyState } from '@/components/EmptyState/EmptyState';
import { IconButton } from '@/components/ui/IconButton';
import { PageHeader } from '@/components/ui/PageHeader';
import {
  THEMES_MIN_DOCUMENTS,
  useClustersQuery,
  useRebuildProgress,
  useRebuildThemes,
} from '@/features/files/hooks/useClustersQuery';
import { useCorpusIdentity } from '@/features/files/hooks/useCorpusIdentity';
import { useCustomCollectionActions } from '@/features/files/hooks/useCustomCollectionsQuery';
import { useIndexedFoldersQuery } from '@/features/files/hooks/useIndexedFoldersQuery';
import {
  openDocumentFile,
  readDocumentText,
  resolveDocumentPath,
  useLibraryDocumentActions,
} from '@/features/files/hooks/useLibraryDocumentActions';
import { useLibraryDocumentsQuery } from '@/features/files/hooks/useLibraryDocumentsQuery';
import { isHttpUrl, isWebDocument, pathBasename, typeBucket } from '@/features/files/model/documentMetadata';
import { filterLibraryDocuments, useFileBrowserStore } from '@/features/files/stores/fileBrowserStore';
import { useRegisterPaletteCommands } from '@/features/palette/hooks/useRegisterPaletteCommands';
import type { PaletteCommand } from '@/features/palette/stores/paletteCommandsStore';
import { ContentViewer } from '@/features/reading/components/ContentViewer';
import { useSpacesQuery } from '@/features/spaces/api/queries';
import { toast } from '@/stores/toastStore';
import { type CustomCollection, type DocumentMetadata, type SortField, type SortOrder } from '@/types/fileBrowser';
import { isSupportedFileType } from '@/utils/fileTypeDetector';
import { handleAsyncEvent } from '@/utils/promiseHandlers';

import { AddToCollectionDialog, CollectionDocumentsDialog, RenameCollectionDialog } from './CollectionDialogs';
import { ContentSearchCache } from './contentSearchCache';
import { ContextMenu } from './ContextMenu';
import { GridView } from './GridView';
import { LibraryRail } from './LibraryRail';
import { LibraryToolbar } from './LibraryToolbar';
import { ListView } from './ListView';
import { RenameDialog } from './RenameDialog';
import { SavedSearchNameDialog, type SavedSearchNameDialogMode } from './SavedSearchNameDialog';
import { SelectionBar } from './SelectionBar';
import { TreeView } from './TreeView';
import { TypeFacets } from './TypeFacets';
import { NeighborhoodPanel } from '../Neighborhood';

const isAbsolutePath = (path: string): boolean =>
  path.startsWith('/') || /^[A-Za-z]:[\\/]/.test(path);

const normalizeCategory = (category: string): string =>
  category.toLowerCase().replace(/[_-]+/g, ' ').trim();

const normalizeSearchToken = (value: string): string => value.trim().toLowerCase();

const NON_TEXT_FILE_TYPES = new Set([
  'png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico',
  'mp3', 'wav', 'flac', 'ogg', 'm4a',
  'mp4', 'mkv', 'mov', 'avi', 'webm',
  'zip', 'rar', '7z', 'tar', 'gz',
]);

/**
 * Which documents the content search reads. Must stay in step with
 * `filterLibraryDocuments`'s `matchesType`, or a facet would narrow the list
 * while the content search kept reading a different set.
 */
const matchesTypeFilter = (doc: DocumentMetadata, activeFilter: string | null): boolean => {
  if (!activeFilter) {
    return true;
  }

  const normalizedCategory = normalizeCategory(doc.category);
  const normalizedFileType = (doc.fileType ?? '').toLowerCase();

  return (
    typeBucket(doc).toLowerCase() === activeFilter ||
    normalizedCategory.includes(activeFilter) ||
    normalizedFileType === activeFilter ||
    (activeFilter === 'web' && normalizedCategory.includes('web article'))
  );
};

const canSearchByContent = (doc: DocumentMetadata): boolean => {
  const normalizedFileType = (doc.fileType ?? '').toLowerCase().trim();
  if (isHttpUrl(doc.filePath)) {
    return false;
  }
  return !NON_TEXT_FILE_TYPES.has(normalizedFileType);
};

const errorMessage = (error: unknown): string => (error instanceof Error ? error.message : String(error));

const plural = (count: number, noun: string): string =>
  `${count.toLocaleString()} ${noun}${count === 1 ? '' : 's'}`;

export function FileBrowser() {
  const { documents, filteredDocuments, refreshFiles, collections: customCollections, collectionsError, refreshCollections } = useLibraryDocumentsQuery();
  const collectionActions = useCustomCollectionActions();
  const indexedFoldersQuery = useIndexedFoldersQuery();
  const viewMode = useFileBrowserStore(state => state.viewMode);
  const setViewMode = useFileBrowserStore(state => state.setViewMode);
  const selectedDocumentIds = useFileBrowserStore(state => state.selectedDocumentIds);
  const selectAll = useFileBrowserStore(state => state.selectAll);
  const clearSelection = useFileBrowserStore(state => state.clearSelection);
  const scope = useFileBrowserStore(state => state.scope);
  const setScope = useFileBrowserStore(state => state.setScope);
  const sourceConnections = useFileBrowserStore(state => state.sourceConnections);
  const searchQuery = useFileBrowserStore(state => state.searchQuery);
  const setSearchQuery = useFileBrowserStore(state => state.setSearchQuery);
  const filterByType = useFileBrowserStore(state => state.filterByType);
  const setFilterByType = useFileBrowserStore(state => state.setFilterByType);
  const filterBySource = useFileBrowserStore(state => state.filterBySource);
  const setFilterBySource = useFileBrowserStore(state => state.setFilterBySource);
  const sortField = useFileBrowserStore(state => state.sortField);
  const sortOrder = useFileBrowserStore(state => state.sortOrder);
  const setSortField = useFileBrowserStore(state => state.setSortField);
  const setSortOrder = useFileBrowserStore(state => state.setSortOrder);
  const groupByDate = useFileBrowserStore(state => state.groupByDate);
  const toggleGroupByDate = useFileBrowserStore(state => state.toggleGroupByDate);
  const savedSearches = useFileBrowserStore(state => state.savedSearches);
  const activeSavedSearchId = useFileBrowserStore(state => state.activeSavedSearchId);
  const createSavedSearch = useFileBrowserStore(state => state.createSavedSearch);
  const applySavedSearch = useFileBrowserStore(state => state.applySavedSearch);
  const updateSavedSearch = useFileBrowserStore(state => state.updateSavedSearch);
  const isContentSearchLoading = useFileBrowserStore(state => state.isContentSearchLoading);
  const setContentSearchMatches = useFileBrowserStore(state => state.setContentSearchMatches);
  const setContentSearchLoading = useFileBrowserStore(state => state.setContentSearchLoading);
  const clearContentSearch = useFileBrowserStore(state => state.clearContentSearch);
  const contextMenuPosition = useFileBrowserStore(state => state.contextMenuPosition);
  const contextMenuDocument = useFileBrowserStore(state => state.contextMenuDocument);
  const openContextMenu = useFileBrowserStore(state => state.openContextMenu);
  const closeContextMenu = useFileBrowserStore(state => state.closeContextMenu);
  const focusedDocumentId = useFileBrowserStore(state => state.focusedDocumentId);
  const setFocusedDocument = useFileBrowserStore(state => state.setFocusedDocument);
  const isNeighborhoodOpen = useFileBrowserStore(state => state.isNeighborhoodOpen);
  const toggleNeighborhood = useFileBrowserStore(state => state.toggleNeighborhood);

  const [isRailOpen, setIsRailOpen] = useState(true);
  const [viewerFilePath, setViewerFilePath] = useState<string | null>(null);
  const [renameDocument, setRenameDocument] = useState<DocumentMetadata | null>(null);
  const [pendingDeleteDoc, setPendingDeleteDoc] = useState<DocumentMetadata | null>(null);
  const [pendingBulkDelete, setPendingBulkDelete] = useState<{ ids: string[]; count: number } | null>(null);
  const [savedSearchNameDialogMode, setSavedSearchNameDialogMode] = useState<SavedSearchNameDialogMode | null>(null);
  const [savedSearchNameDialogValue, setSavedSearchNameDialogValue] = useState('');
  const [savedSearchNameDialogTargetId, setSavedSearchNameDialogTargetId] = useState<string | null>(null);
  const bulkDeleteCancelled = useRef(false);
  const [deleteProgress, setDeleteProgress] = useState(0);
  const documentActions = useLibraryDocumentActions();
  const isDeleting = documentActions.deleteDocuments.isPending;
  const isAssigningSpace = documentActions.addToSpace.isPending;
  const spacesQuery = useSpacesQuery();
  // Open spaces first, then archived ones, each by name.
  const spaces = useMemo(
    () => (spacesQuery.data ?? [])
      .slice()
      .sort((a, b) => Number(a.isArchived) - Number(b.isArchived) || a.name.localeCompare(b.name)),
    [spacesQuery.data]
  );
  const isLoadingSpaces = spacesQuery.isLoading;
  const [collectionSelection, setCollectionSelection] = useState<string[] | null>(null);
  const [addDocumentsCollectionId, setAddDocumentsCollectionId] = useState<string | null>(null);
  const [renameCollection, setRenameCollection] = useState<CustomCollection | null>(null);
  const [pendingDeleteCollection, setPendingDeleteCollection] = useState<CustomCollection | null>(null);

  const contentCacheRef = useRef(new ContentSearchCache());
  const contentSearchSequenceRef = useRef(0);

  const navigate = useNavigate();

  useEffect(() => {
    const currentIds = new Set(documents.map(doc => doc.id));

    contentCacheRef.current.retain(currentIds);
  }, [documents]);

  const indexedFolders = useMemo(() => indexedFoldersQuery.data ?? [], [indexedFoldersQuery.data]);

  const sourceCounts = useMemo(() => {
    let web = 0;
    let local = 0;
    for (const doc of documents) {
      if (isWebDocument(doc)) {
        web += 1;
      } else {
        local += 1;
      }
    }
    return { all: documents.length, web, local };
  }, [documents]);

  // Counted over the unfiltered list on purpose: the facet line is a readout of
  // the whole library, so the numbers do not move while a facet is active.
  const identity = useCorpusIdentity(documents);

  const documentsById = useMemo(
    () => new Map(documents.map((doc) => [doc.id, doc])),
    [documents]
  );
  const activeCollection = scope.kind === 'collection'
    ? customCollections.find(collection => collection.id === scope.id) ?? null
    : null;
  const addDocumentsCollection = customCollections.find(collection => collection.id === addDocumentsCollectionId);
  const collectionCounts = useMemo(() => new Map(customCollections.map(collection => [
    collection.id,
    filterLibraryDocuments(documents, {
      searchQuery: '', filterByType: null, filterBySource: 'all', contentSearchMatches: new Set(),
      customCollections, scope: { kind: 'collection', id: collection.id },
    }).length,
  ])), [customCollections, documents]);

  useEffect(() => {
    if (scope.kind === 'collection' && !activeCollection) setScope({ kind: 'all' });
  }, [activeCollection, scope, setScope]);
  const focusedDoc = useMemo(
    () => (focusedDocumentId ? documentsById.get(focusedDocumentId) ?? null : null),
    [documentsById, focusedDocumentId]
  );

  const themesEnabled = documents.length >= THEMES_MIN_DOCUMENTS;
  const themesQuery = useClustersQuery(themesEnabled);
  const themes = useMemo(() => themesQuery.data ?? [], [themesQuery.data]);
  const rebuildThemes = useRebuildThemes();
  const isFindingThemes = rebuildThemes.isPending;
  const themeProgress = useRebuildProgress(isFindingThemes);
  const hasRunThemes = themes.length > 0;

  const headerMeta = useMemo(
    () =>
      [
        plural(documents.length, 'document'),
        `${sourceCounts.local.toLocaleString()} local`,
        `${sourceCounts.web.toLocaleString()} web`,
        plural(indexedFolders.length, 'folder'),
      ].join(' · '),
    [documents.length, indexedFolders.length, sourceCounts.local, sourceCounts.web]
  );

  const scopeName = useMemo(() => {
    if (scope.kind === 'folder') {
      return pathBasename(scope.path);
    }
    if (scope.kind === 'collection') {
      return customCollections.find((collection) => collection.id === scope.id)?.name ?? 'Collection';
    }
    if (scope.kind === 'theme') {
      return themes.find((theme) => theme.id === scope.id)?.label ?? 'Theme';
    }
    if (filterBySource === 'web') return 'Web';
    if (filterBySource === 'local') return 'Local';
    return 'All documents';
  }, [customCollections, filterBySource, scope, themes]);

  const normalizedSearchQuery = useMemo(() => normalizeSearchToken(searchQuery), [searchQuery]);
  const activeFilter = useMemo(() => normalizeSearchToken(filterByType ?? '') || null, [filterByType]);

  const contentSearchScope = useMemo(
    () => documents.filter(doc => matchesTypeFilter(doc, activeFilter)),
    [activeFilter, documents]
  );

  useEffect(() => {
    const requestId = contentSearchSequenceRef.current + 1;
    contentSearchSequenceRef.current = requestId;

    if (!normalizedSearchQuery) {
      clearContentSearch();
      return;
    }

    const documentsToCheck = contentSearchScope.filter((doc) => {
      const fileNameMatches = normalizeSearchToken(doc.fileName).includes(normalizedSearchQuery);
      return !fileNameMatches && canSearchByContent(doc);
    });

    if (documentsToCheck.length === 0) {
      setContentSearchMatches([]);
      setContentSearchLoading(false);
      return;
    }

    let cancelled = false;

    const timeoutId = window.setTimeout(() => {
      void (async () => {
        setContentSearchLoading(true);

        const contentMatches = new Set<string>();
        const batchSize = 6;

        for (let start = 0; start < documentsToCheck.length; start += batchSize) {
          if (cancelled || requestId !== contentSearchSequenceRef.current) {
            return;
          }

          const batch = documentsToCheck.slice(start, start + batchSize);
          const batchResults = await Promise.all(batch.map(async (doc) => {
            const version = JSON.stringify([doc.filePath, doc.modifiedAt, doc.indexedAt]);
            const cachedContent = contentCacheRef.current.get(doc.id, version);
            if (typeof cachedContent === 'string') {
              return cachedContent.includes(normalizedSearchQuery) ? doc.id : null;
            }

            const result = await readDocumentText(doc.filePath);
            if (!result.ok) {
              return null;
            }

            const normalizedContent = result.data.toLowerCase();
            contentCacheRef.current.set(doc.id, version, normalizedContent);

            return normalizedContent.includes(normalizedSearchQuery) ? doc.id : null;
          }));

          for (const docId of batchResults) {
            if (docId) {
              contentMatches.add(docId);
            }
          }
        }

        if (!cancelled && requestId === contentSearchSequenceRef.current) {
          setContentSearchMatches(contentMatches);
          setContentSearchLoading(false);
        }
      })().catch(() => {
        if (!cancelled && requestId === contentSearchSequenceRef.current) {
          setContentSearchMatches([]);
          setContentSearchLoading(false);
        }
      });
    }, 220);

    return () => {
      cancelled = true;
      window.clearTimeout(timeoutId);
    };
  }, [
    clearContentSearch,
    contentSearchScope,
    normalizedSearchQuery,
    setContentSearchLoading,
    setContentSearchMatches,
  ]);

  const handleFileOpen = useCallback(async (doc: DocumentMetadata) => {
    setFocusedDocument(doc.id);
    if (isWebDocument(doc)) {
      if (isHttpUrl(doc.filePath)) {
        try {
          await openExternal(doc.filePath);
        } catch (error) {
          toast.error('Failed to open URL', {
            message: error instanceof Error ? error.message : String(error),
          });
        }
        return;
      }

      const webOpen = await openDocumentFile(doc.id);
      if (!webOpen.ok) {
        toast.error('Failed to open web article', { message: webOpen.error });
        return;
      }

      if (webOpen.data.action === 'render_internal') {
        setViewerFilePath(webOpen.data.contentPath);
      }
      return;
    }

    if (isSupportedFileType(doc.filePath)) {
      if (isAbsolutePath(doc.filePath)) {
        setViewerFilePath(doc.filePath);
        return;
      }

      const resolvedPath = await resolveDocumentPath(doc);
      if (resolvedPath) {
        setViewerFilePath(resolvedPath);
        return;
      }

      toast.error('Failed to resolve file path', { message: 'No file path for this document.' });
      return;
    }

    const openById = await openDocumentFile(doc.id);
    if (!openById.ok) {
      toast.error('Failed to open file', { message: openById.error });
    }
  }, [setFocusedDocument]);

  const askAbout = useCallback(
    (doc: DocumentMetadata) => {
      navigate(`/chat?${new URLSearchParams({ new: '1', documentId: doc.id }).toString()}`);
    },
    [navigate]
  );

  const { reindex, removeFromIndex: unindex, addToSpace, deleteDocuments } = documentActions;

  const reindexDocument = useCallback(
    async (doc: DocumentMetadata) => {
      try {
        await reindex.mutateAsync(doc);
        toast.success(`Reindexing ${doc.fileName}`);
      } catch (error) {
        toast.error(`Couldn't reindex ${doc.fileName}`, { message: errorMessage(error) });
      }
    },
    [reindex]
  );

  const removeFromIndex = useCallback(
    async (doc: DocumentMetadata) => {
      try {
        await unindex.mutateAsync(doc);
        toast.success(`Removed ${doc.fileName} from the index`);
      } catch (error) {
        toast.error(`Couldn't remove ${doc.fileName} from the index`, { message: errorMessage(error) });
      }
    },
    [unindex]
  );

  const handleFindThemes = useCallback(() => {
    rebuildThemes.mutate();
  }, [rebuildThemes]);

  const openConversation = useCallback(
    (conversationId: string) => {
      navigate(`/chat?${new URLSearchParams({ conversationId }).toString()}`);
    },
    [navigate]
  );

  const openDocumentById = useCallback(
    (documentId: string) => {
      const doc = documentsById.get(documentId);
      if (doc) void handleFileOpen(doc);
    },
    [documentsById, handleFileOpen]
  );

  // A theme that vanished in the last run must not leave the list empty and
  // unexplained; fall back to the whole library.
  useEffect(() => {
    if (scope.kind === 'theme' && themes.length > 0 && !themes.some((theme) => theme.id === scope.id)) {
      setScope({ kind: 'all' });
    }
  }, [scope, setScope, themes]);

  const paletteCommands = useMemo<PaletteCommand[]>(
    () => [
      {
        id: 'library.askAboutDocument',
        label: 'Ask about this document',
        group: 'Library',
        icon: MessageSquare,
        enabled: focusedDoc !== null,
        run: () => {
          if (focusedDoc) askAbout(focusedDoc);
        },
      },
      {
        id: 'library.reindexFile',
        label: 'Reindex this file',
        group: 'Library',
        icon: RefreshCw,
        enabled: focusedDoc !== null && !isWebDocument(focusedDoc),
        run: () => {
          if (focusedDoc) void reindexDocument(focusedDoc);
        },
      },
      {
        id: 'library.removeFromIndex',
        label: 'Remove this file from the index',
        group: 'Library',
        icon: EyeOff,
        enabled: focusedDoc !== null && !isWebDocument(focusedDoc),
        run: () => {
          if (focusedDoc) void removeFromIndex(focusedDoc);
        },
      },
      {
        id: 'library.toggleRelated',
        label: isNeighborhoodOpen ? 'Hide related documents' : 'Show related documents',
        group: 'Library',
        icon: PanelRight,
        enabled: focusedDoc !== null,
        run: toggleNeighborhood,
      },
      {
        id: 'library.findThemes',
        label: hasRunThemes ? 'Refresh themes' : 'Find themes in vault',
        group: 'Library',
        icon: Layers,
        enabled: themesEnabled && !isFindingThemes,
        run: handleFindThemes,
      },
    ],
    [
      askAbout,
      focusedDoc,
      handleFindThemes,
      hasRunThemes,
      isFindingThemes,
      isNeighborhoodOpen,
      reindexDocument,
      removeFromIndex,
      themesEnabled,
      toggleNeighborhood,
    ]
  );
  useRegisterPaletteCommands(paletteCommands);

  const handleContextMenu = useCallback(
    (event: React.MouseEvent, doc: DocumentMetadata) => {
      event.preventDefault();
      const rect = event.currentTarget.getBoundingClientRect();
      openContextMenu({
        x: event.clientX || rect.left,
        y: event.clientY || rect.bottom,
      }, doc);
    },
    [openContextMenu]
  );

  const handleRefresh = useCallback(async () => {
    await refreshFiles();
  }, [refreshFiles]);

  const handleAddFolder = useCallback(() => {
    navigate('/ingest');
  }, [navigate]);

  const handleCreateCollection = useCallback(async (name: string) => {
    let collectionId: string | null;
    try { collectionId = await collectionActions.create(name); }
    catch (error) { toast.error('Collection could not be saved', { message: error instanceof Error ? error.message : String(error) }); return; }
    if (!collectionId) {
      toast.warning('Collection not created', { message: 'That name is already in use.' });
      return;
    }
    const ids = Array.from(selectedDocumentIds);
    if (ids.length > 0) {
      try { await collectionActions.addDocuments(collectionId, ids); }
      catch (error) { toast.error('Documents could not be added to the collection', { message: error instanceof Error ? error.message : String(error) }); }
    }
    else setAddDocumentsCollectionId(collectionId);
    setSearchQuery('');
    setFilterByType(null);
    setFilterBySource('all');
    setScope({ kind: 'collection', id: collectionId });
  }, [collectionActions, selectedDocumentIds, setFilterBySource, setFilterByType, setScope, setSearchQuery]);

  const handleRemoveFromCollection = useCallback(async (ids: string[]) => {
    if (activeCollection?.kind !== 'manual') return;
    try { await collectionActions.removeDocuments(activeCollection.id, ids); }
    catch (error) { toast.error('Documents could not be removed from the collection', { message: error instanceof Error ? error.message : String(error) }); return; }
    clearSelection();
    toast.success(`Removed from ${activeCollection.name}`, { message: 'Documents are still in your Library.' });
  }, [activeCollection, clearSelection, collectionActions]);

  const handleSnapshotSelection = useCallback(async () => {
    const docIds = Array.from(selectedDocumentIds);
    if (docIds.length === 0) {
      return;
    }

    const label = searchQuery.trim() || new Date().toLocaleDateString();
    let collectionId: string | null;
    try { collectionId = await collectionActions.create(`Snapshot: ${label}`, null, docIds); }
    catch (error) { toast.error('Snapshot could not be saved', { message: error instanceof Error ? error.message : String(error) }); return; }
    if (!collectionId) {
      toast.warning('Snapshot not created', { message: 'That name is already in use.' });
      return;
    }

    toast.success(`${plural(docIds.length, 'document')} captured`);
    clearSelection();
  }, [clearSelection, collectionActions, searchQuery, selectedDocumentIds]);

  const handleSortChange = useCallback((field: SortField, order: SortOrder) => {
    setSortField(field);
    setSortOrder(order);
  }, [setSortField, setSortOrder]);

  const openSavedSearchNameDialog = useCallback((
    mode: SavedSearchNameDialogMode,
    value: string,
    targetId: string | null = null
  ) => {
    setSavedSearchNameDialogMode(mode);
    setSavedSearchNameDialogValue(value);
    setSavedSearchNameDialogTargetId(targetId);
  }, []);

  const closeSavedSearchNameDialog = useCallback(() => {
    setSavedSearchNameDialogMode(null);
    setSavedSearchNameDialogValue('');
    setSavedSearchNameDialogTargetId(null);
  }, []);

  const submitSavedSearchNameDialog = useCallback(() => {
    const name = savedSearchNameDialogValue.trim();
    if (!name) {
      toast.warning('Search name cannot be empty');
      return;
    }

    if (savedSearchNameDialogMode === 'create') {
      const searchId = createSavedSearch(name);
      if (!searchId) {
        toast.warning('Saved search not created', {
          message: 'That name is already in use, or nothing is being searched.',
        });
        return;
      }
      closeSavedSearchNameDialog();
      return;
    }

    if (savedSearchNameDialogMode === 'rename' && savedSearchNameDialogTargetId) {
      updateSavedSearch(savedSearchNameDialogTargetId, { name });
      closeSavedSearchNameDialog();
    }
  }, [
    closeSavedSearchNameDialog,
    createSavedSearch,
    savedSearchNameDialogMode,
    savedSearchNameDialogTargetId,
    savedSearchNameDialogValue,
    updateSavedSearch,
  ]);

  const handleCreateSavedSearch = useCallback((name: string) => {
    const hasCriteria = Boolean(searchQuery.trim()) || Boolean(activeFilter) || filterBySource !== 'all';
    if (!hasCriteria) {
      toast.warning('Nothing to save', { message: 'Search or filter first.' });
      return;
    }

    const searchId = createSavedSearch(name);
    if (!searchId) {
      toast.warning('Saved search not created', { message: 'That name is already in use.' });
    }
  }, [activeFilter, createSavedSearch, filterBySource, searchQuery]);

  const handleBulkAssignToSpace = useCallback(async (spaceId: string) => {
    const selectedIds = Array.from(selectedDocumentIds);
    if (selectedIds.length === 0) {
      return;
    }

    try {
      await addToSpace.mutateAsync({ documentIds: selectedIds, spaceId });
      const spaceName = spaces.find((space) => space.id === spaceId)?.name ?? 'space';
      toast.success(`${plural(selectedIds.length, 'document')} added to ${spaceName}`);
      clearSelection();
    } catch (error) {
      toast.error('Failed to add documents to space', { message: errorMessage(error) });
    }
  }, [addToSpace, clearSelection, selectedDocumentIds, spaces]);

  const executeBulkDelete = useCallback(async () => {
    if (!pendingBulkDelete || isDeleting) return;

    bulkDeleteCancelled.current = false;
    setDeleteProgress(0);

    try {
      const { deleted, errors } = await deleteDocuments.mutateAsync({
        ids: pendingBulkDelete.ids,
        shouldStop: () => bulkDeleteCancelled.current,
        onProgress: setDeleteProgress,
      });

      if (deleted > 0) {
        toast.success(`${plural(deleted, 'document')} deleted`);
        clearSelection();
      }

      if (errors.length > 0) {
        toast.error(`Failed to delete ${plural(errors.length, 'document')}`, {
          message: errors.slice(0, 3).join(', ') + (errors.length > 3 ? '…' : ''),
        });
      }

    } catch (error) {
      toast.error('Deletion stopped', { message: errorMessage(error) });
    } finally {
      setPendingBulkDelete(null);
    }
  }, [pendingBulkDelete, isDeleting, clearSelection, deleteDocuments]);

  const handleRename = useCallback((doc: DocumentMetadata) => {
    setRenameDocument(doc);
  }, []);

  const handleRenameSuccess = useCallback(async () => {
    await refreshFiles();
  }, [refreshFiles]);

  const handleDelete = useCallback(async (doc: DocumentMetadata) => {
    setPendingDeleteDoc(doc);
  }, []);

  const executeDelete = useCallback(async () => {
    if (!pendingDeleteDoc) return;

    const { errors } = await deleteDocuments.mutateAsync({ ids: [pendingDeleteDoc.id] });
    if (errors.length > 0) throw new Error(errors[0]);
    toast.success('Document deleted');
    setPendingDeleteDoc(null);
  }, [deleteDocuments, pendingDeleteDoc]);

  const viewProps = {
    onFileOpen: handleAsyncEvent(handleFileOpen),
    onContextMenu: handleContextMenu,
    onRename: handleRename,
    onDelete: handleAsyncEvent(handleDelete),
    onAddFolder: handleAddFolder,
  };

  return (
    <main className="h-full overflow-y-auto bg-bg">
      <div
        className={`mx-auto flex h-full w-full flex-col px-6 pb-10 pt-10 ${
          isNeighborhoodOpen && focusedDoc ? 'max-w-[1360px]' : 'max-w-[1100px]'
        }`}
      >
        <PageHeader
          title="Library"
          meta={headerMeta}
          actions={
            <>
              <IconButton
                label="List view"
                active={viewMode === 'list'}
                onClick={() => setViewMode('list')}
              >
                <List strokeWidth={1.75} />
              </IconButton>
              <IconButton
                label="Grid view"
                active={viewMode === 'grid'}
                onClick={() => setViewMode('grid')}
              >
                <LayoutGrid strokeWidth={1.75} />
              </IconButton>
              <IconButton
                label="Tree view"
                active={viewMode === 'tree'}
                onClick={() => setViewMode('tree')}
              >
                <ListTree strokeWidth={1.75} />
              </IconButton>
              <IconButton
                label="Related"
                active={isNeighborhoodOpen}
                disabled={focusedDocumentId === null}
                onClick={toggleNeighborhood}
              >
                <PanelRight strokeWidth={1.75} />
              </IconButton>
              <IconButton label="Refresh" onClick={handleAsyncEvent(handleRefresh)}>
                <RefreshCw strokeWidth={1.75} />
              </IconButton>
            </>
          }
        />

        <LibraryToolbar
          isRailOpen={isRailOpen}
          onToggleRail={() => setIsRailOpen((open) => !open)}
          searchQuery={searchQuery}
          onSearchQueryChange={setSearchQuery}
          isContentSearchLoading={isContentSearchLoading}
          sourceCounts={sourceCounts}
          filterBySource={filterBySource}
          onFilterBySourceChange={setFilterBySource}
          sortField={sortField}
          sortOrder={sortOrder}
          onSortChange={handleSortChange}
          groupByDate={groupByDate}
          onToggleGroupByDate={toggleGroupByDate}
        />

        {collectionsError ? (
          <div role="alert" className="mt-3 flex items-center justify-between rounded-md border border-danger/30 bg-danger/5 px-3 py-2 text-sm text-danger">
            <span>Your collections could not be loaded: {collectionsError}</span>
            <button type="button" className="underline" onClick={() => { void refreshCollections(); }}>Retry</button>
          </div>
        ) : null}

        <div className="flex min-h-0 flex-1 gap-6">
          {isRailOpen ? (
            <LibraryRail
              scope={scope}
              onScopeChange={setScope}
              collections={customCollections}
              onCreateCollection={handleCreateCollection}
              onRenameCollection={setRenameCollection}
              onDeleteCollection={setPendingDeleteCollection}
              collectionCounts={collectionCounts}
              savedSearches={savedSearches}
              activeSavedSearchId={activeSavedSearchId}
              onApplySavedSearch={applySavedSearch}
              onRenameSavedSearch={(searchId, name) =>
                openSavedSearchNameDialog('rename', name, searchId)
              }
              onCreateSavedSearch={handleCreateSavedSearch}
              sources={sourceConnections}
              themes={themes}
              themesEnabled={themesEnabled}
              hasRunThemes={hasRunThemes}
              isFindingThemes={isFindingThemes}
              themeProgress={themeProgress}
              onFindThemes={handleFindThemes}
            />
          ) : null}

          <div className="flex min-h-0 min-w-0 flex-1 flex-col">
            <div className="flex items-baseline gap-2 pb-3">
              <h2 className="min-w-0 truncate text-lg font-medium text-text-secondary">{scopeName}</h2>
              <span className="text-sm tabular-nums text-text-muted">
                {filteredDocuments.length.toLocaleString()}
              </span>
              {activeCollection?.kind === 'manual' ? (
                <button type="button" onClick={() => setAddDocumentsCollectionId(activeCollection.id)} className="ml-auto shrink-0 rounded-sm border border-border-default px-3 py-1.5 text-sm text-text-primary hover:bg-surface">Add documents</button>
              ) : null}
            </div>
            <TypeFacets
              buckets={identity.buckets}
              activeType={filterByType}
              onSelect={setFilterByType}
            />

            {selectedDocumentIds.size > 0 ? (
              <SelectionBar
                selectedCount={selectedDocumentIds.size}
                spaces={spaces}
                isLoadingSpaces={isLoadingSpaces}
                isAssigningSpace={isAssigningSpace}
                onAssignToSpace={handleAsyncEvent(handleBulkAssignToSpace)}
                onAddToCollection={() => setCollectionSelection(Array.from(selectedDocumentIds))}
                onRemoveFromCollection={activeCollection?.kind === 'manual' && activeCollection.documentIds.some(id => selectedDocumentIds.has(id))
                  ? () => handleRemoveFromCollection(Array.from(selectedDocumentIds)) : undefined}
                onSnapshot={handleSnapshotSelection}
                onDelete={() =>
                  setPendingBulkDelete({
                    ids: Array.from(selectedDocumentIds),
                    count: selectedDocumentIds.size,
                  })
                }
                onSelectAll={() => selectAll(filteredDocuments.map((doc) => doc.id))}
                onClear={clearSelection}
              />
            ) : null}

            <div className="min-h-0 flex-1">
              {activeCollection && collectionCounts.get(activeCollection.id) === 0 ? (
                <EmptyState
                  title="This collection is empty."
                  description={activeCollection.kind === 'manual' ? 'Add documents from your Library to get started.' : 'The documents in this snapshot are no longer in your Library.'}
                  action={activeCollection.kind === 'manual' ? { label: 'Choose documents', onClick: () => setAddDocumentsCollectionId(activeCollection.id) } : undefined}
                />
              ) : <>
              {viewMode === 'tree' && <TreeView {...viewProps} />}
              {viewMode === 'list' && <ListView {...viewProps} />}
              {viewMode === 'grid' && (
                <GridView
                  onFileOpen={viewProps.onFileOpen}
                  onContextMenu={viewProps.onContextMenu}
                  onAddFolder={viewProps.onAddFolder}
                />
              )}
              </>}
            </div>
          </div>

          {isNeighborhoodOpen && focusedDoc ? (
            <NeighborhoodPanel
              documentId={focusedDoc.id}
              documentTitle={focusedDoc.fileName}
              documentsById={documentsById}
              onOpenDocument={openDocumentById}
              onOpenConversation={openConversation}
              onClose={toggleNeighborhood}
            />
          ) : null}
        </div>
      </div>

      {contextMenuPosition && contextMenuDocument && (
        <ContextMenu
          doc={contextMenuDocument}
          position={contextMenuPosition}
          onClose={closeContextMenu}
          onViewInRecall={(doc) => {
            void handleFileOpen(doc);
          }}
          onRename={handleRename}
          onDelete={handleAsyncEvent(handleDelete)}
          onAddToCollection={doc => setCollectionSelection([doc.id])}
          onRemoveFromCollection={activeCollection?.kind === 'manual' && activeCollection.documentIds.includes(contextMenuDocument.id)
            ? doc => handleRemoveFromCollection([doc.id]) : undefined}
        />
      )}

      {renameDocument && (
        <RenameDialog
          document={renameDocument}
          onClose={() => setRenameDocument(null)}
          onSuccess={handleAsyncEvent(handleRenameSuccess)}
        />
      )}

      <SavedSearchNameDialog
        open={savedSearchNameDialogMode !== null}
        mode={savedSearchNameDialogMode}
        value={savedSearchNameDialogValue}
        onChangeValue={setSavedSearchNameDialogValue}
        onClose={closeSavedSearchNameDialog}
        onSubmit={submitSavedSearchNameDialog}
      />

      <ContentViewer filePath={viewerFilePath} onClose={() => setViewerFilePath(null)} />

      {collectionSelection ? <AddToCollectionDialog documentIds={collectionSelection} onClose={() => setCollectionSelection(null)} /> : null}
      {addDocumentsCollection ? <CollectionDocumentsDialog collection={addDocumentsCollection} documents={documents} onClose={() => setAddDocumentsCollectionId(null)} /> : null}
      {renameCollection ? <RenameCollectionDialog collection={renameCollection} onClose={() => setRenameCollection(null)} /> : null}
      <ConfirmDialog
        isOpen={pendingDeleteCollection !== null}
        title="Delete collection?"
        message={`Delete “${pendingDeleteCollection?.name ?? ''}” and any collections inside it? Your documents will remain in the Library.`}
        confirmLabel="Delete collection"
        variant="danger"
        onConfirm={async () => {
          if (!pendingDeleteCollection) return;
          try { await collectionActions.deleteCollection(pendingDeleteCollection.id); setPendingDeleteCollection(null); }
          catch (error) { toast.error('Collection could not be deleted', { message: error instanceof Error ? error.message : String(error) }); }
        }}
        onCancel={() => setPendingDeleteCollection(null)}
      />

      <ConfirmDialog
        isOpen={pendingDeleteDoc !== null}
        title="Delete document?"
        message={`Delete "${pendingDeleteDoc?.fileName ?? 'this document'}"? The document and its embeddings are removed from the index.`}
        confirmLabel="Delete"
        cancelLabel="Cancel"
        variant="danger"
        confirmDisabled={isDeleting}
        onConfirm={executeDelete}
        onCancel={() => setPendingDeleteDoc(null)}
      />

      <ConfirmDialog
        isOpen={pendingBulkDelete !== null}
        title="Delete documents?"
        message={isDeleting
          ? `Processed ${deleteProgress} of ${pendingBulkDelete?.count ?? 0} documents. Cancel stops after the current document finishes.`
          : `Delete ${plural(pendingBulkDelete?.count ?? 0, 'document')}? They and their embeddings are removed from the index.`}
        confirmLabel="Delete"
        cancelLabel="Cancel"
        variant="danger"
        confirmDisabled={isDeleting}
        onConfirm={executeBulkDelete}
        allowCancelWhileLoading
        onCancel={() => {
          bulkDeleteCancelled.current = true;
          setPendingBulkDelete(null);
        }}
      />
    </main>
  );
}
