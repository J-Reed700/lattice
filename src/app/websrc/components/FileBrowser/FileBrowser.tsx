/**
 * Library — the corpus surface.
 *
 * Header (title + one data line + view toggles), one toolbar row, an optional
 * rail of the things the corpus is organised by, and the documents themselves
 * as hairline rows. No cards, no badges, no explanatory copy.
 */

import { useState, useCallback, useEffect, useMemo, useRef } from 'react';

import { useQueryClient } from '@tanstack/react-query';
import { open as openExternal } from '@tauri-apps/plugin-shell';
import { EyeOff, LayoutGrid, Layers, List, ListTree, MessageSquare, PanelRight, RefreshCw } from 'lucide-react';
import { useNavigate } from 'react-router';

import { ContextMenu } from './ContextMenu';
import { isHttpUrl, isWebDocument, pathBasename, typeBucket } from './docMeta';
import { GridView } from './GridView';
import { useCorpusIdentity } from './hooks/useCorpusIdentity';
import { LibraryRail } from './LibraryRail';
import { LibraryToolbar } from './LibraryToolbar';
import { ListView } from './ListView';
import { RenameDialog } from './RenameDialog';
import { SavedSearchNameDialog, type SavedSearchNameDialogMode } from './SavedSearchNameDialog';
import { SelectionBar } from './SelectionBar';
import { TreeView } from './TreeView';
import { TypeFacets } from './TypeFacets';
import {
  THEMES_MIN_DOCUMENTS,
  useClustersQuery,
  useRebuildProgress,
  useRebuildThemes,
} from '../../hooks/queries/useClustersQuery';
import { useIndexedFoldersQuery } from '../../hooks/queries/useIndexedFoldersQuery';
import {
  LIBRARY_DOCUMENTS_QUERY_KEY,
  useLibraryDocumentsQuery,
} from '../../hooks/queries/useLibraryDocumentsQuery';
import { useRegisterPaletteCommands } from '../../hooks/useRegisterPaletteCommands';
import VaultAPI from '../../lib/api';
import { useFileBrowserStore } from '../../stores/fileBrowserStore';
import { toast } from '../../stores/toastStore';
import { type ConversationSpaceDto } from '../../types';
import { type DocumentMetadata, type SortField, type SortOrder } from '../../types/fileBrowser';
import { isSupportedFileType } from '../../utils/fileTypeDetector';
import { handleAsyncEvent } from '../../utils/promiseHandlers';
import { ConfirmDialog } from '../ConfirmDialog';
import { ContentViewer } from '../ContentViewer';
import { NeighborhoodPanel } from '../Neighborhood';
import { IconButton } from '../ui/IconButton';
import { PageHeader } from '../ui/PageHeader';

import type { PaletteCommand } from '../../stores/paletteCommandsStore';

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

const plural = (count: number, noun: string): string =>
  `${count.toLocaleString()} ${noun}${count === 1 ? '' : 's'}`;

export function FileBrowser() {
  const { documents, filteredDocuments, refreshFiles } = useLibraryDocumentsQuery();
  const indexedFoldersQuery = useIndexedFoldersQuery();
  const viewMode = useFileBrowserStore(state => state.viewMode);
  const setViewMode = useFileBrowserStore(state => state.setViewMode);
  const selectedDocumentIds = useFileBrowserStore(state => state.selectedDocumentIds);
  const selectAll = useFileBrowserStore(state => state.selectAll);
  const clearSelection = useFileBrowserStore(state => state.clearSelection);
  const scope = useFileBrowserStore(state => state.scope);
  const setScope = useFileBrowserStore(state => state.setScope);
  const customCollections = useFileBrowserStore(state => state.customCollections);
  const createCustomCollection = useFileBrowserStore(state => state.createCustomCollection);
  const createSnapshotCollection = useFileBrowserStore(state => state.createSnapshotCollection);
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
  const [isDeleting, setIsDeleting] = useState(false);
  const [spaces, setSpaces] = useState<ConversationSpaceDto[]>([]);
  const [isLoadingSpaces, setIsLoadingSpaces] = useState(false);
  const [isAssigningSpace, setIsAssigningSpace] = useState(false);

  const contentCacheRef = useRef<Map<string, string>>(new Map());
  const contentReadFailuresRef = useRef<Set<string>>(new Set());
  const contentSearchSequenceRef = useRef(0);

  const navigate = useNavigate();
  const queryClient = useQueryClient();

  useEffect(() => {
    let cancelled = false;

    const loadSpaces = async () => {
      setIsLoadingSpaces(true);
      const result = await VaultAPI.listConversationSpaces();

      if (cancelled) return;

      if (result.ok) {
        setSpaces(
          result.data
            .slice()
            .sort((a, b) => Number(a.isArchived) - Number(b.isArchived) || a.name.localeCompare(b.name))
        );
      } else {
        setSpaces([]);
      }

      setIsLoadingSpaces(false);
    };

    void loadSpaces();

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const currentIds = new Set(documents.map(doc => doc.id));

    for (const id of contentCacheRef.current.keys()) {
      if (!currentIds.has(id)) {
        contentCacheRef.current.delete(id);
      }
    }

    for (const id of contentReadFailuresRef.current) {
      if (!currentIds.has(id)) {
        contentReadFailuresRef.current.delete(id);
      }
    }
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
            const cachedContent = contentCacheRef.current.get(doc.id);
            if (typeof cachedContent === 'string') {
              return cachedContent.includes(normalizedSearchQuery) ? doc.id : null;
            }

            if (contentReadFailuresRef.current.has(doc.id)) {
              return null;
            }

            const result = await VaultAPI.readFileContent(doc.filePath);
            if (!result.ok) {
              contentReadFailuresRef.current.add(doc.id);
              return null;
            }

            const normalizedContent = result.data.toLowerCase();
            contentCacheRef.current.set(doc.id, normalizedContent);

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

      const webOpen = await VaultAPI.openFileById(doc.id);
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

      const resolvedPath = await VaultAPI.getFilePathById(doc.id);
      if (resolvedPath.ok) {
        setViewerFilePath(resolvedPath.data);
        return;
      }

      toast.error('Failed to resolve file path', { message: resolvedPath.error });
      return;
    }

    const openById = await VaultAPI.openFileById(doc.id);
    if (!openById.ok) {
      toast.error('Failed to open file', { message: openById.error });
    }
  }, [setFocusedDocument]);

  const invalidateLibrary = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: LIBRARY_DOCUMENTS_QUERY_KEY });
  }, [queryClient]);

  const askAbout = useCallback(
    (doc: DocumentMetadata) => {
      navigate(`/chat?${new URLSearchParams({ new: '1', documentId: doc.id }).toString()}`);
    },
    [navigate]
  );

  /** An absolute local path, resolving through the backend when needed. */
  const resolveDocumentPath = useCallback(async (doc: DocumentMetadata): Promise<string | null> => {
    if (!isHttpUrl(doc.filePath) && isAbsolutePath(doc.filePath)) return doc.filePath;
    const result = await VaultAPI.getFilePathById(doc.id);
    return result.ok ? result.data : null;
  }, []);

  const reindexDocument = useCallback(
    async (doc: DocumentMetadata) => {
      const path = await resolveDocumentPath(doc);
      if (!path) {
        toast.error(`Couldn't reindex ${doc.fileName}`, { message: 'No file path for this document.' });
        return;
      }
      const result = await VaultAPI.reindexFile(path);
      if (result.ok) {
        toast.success(`Reindexing ${doc.fileName}`);
      } else {
        toast.error(`Couldn't reindex ${doc.fileName}`, { message: result.error });
      }
    },
    [resolveDocumentPath]
  );

  const removeFromIndex = useCallback(
    async (doc: DocumentMetadata) => {
      const path = await resolveDocumentPath(doc);
      if (!path) {
        toast.error(`Couldn't remove ${doc.fileName} from the index`, {
          message: 'No file path for this document.',
        });
        return;
      }
      const result = await VaultAPI.removeIndexedFile(path);
      if (result.ok) {
        toast.success(`Removed ${doc.fileName} from the index`);
        invalidateLibrary();
      } else {
        toast.error(`Couldn't remove ${doc.fileName} from the index`, { message: result.error });
      }
    },
    [invalidateLibrary, resolveDocumentPath]
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
      openContextMenu({ x: event.clientX, y: event.clientY }, doc);
    },
    [openContextMenu]
  );

  const handleRefresh = useCallback(async () => {
    await refreshFiles();
  }, [refreshFiles]);

  const handleAddFolder = useCallback(() => {
    navigate('/ingest');
  }, [navigate]);

  const handleCreateCollection = useCallback((name: string) => {
    const collectionId = createCustomCollection(name);
    if (!collectionId) {
      toast.warning('Collection not created', { message: 'That name is already in use.' });
      return;
    }
    setScope({ kind: 'collection', id: collectionId });
  }, [createCustomCollection, setScope]);

  const handleSnapshotSelection = useCallback(() => {
    const docIds = Array.from(selectedDocumentIds);
    if (docIds.length === 0) {
      return;
    }

    const label = searchQuery.trim() || new Date().toLocaleDateString();
    const collectionId = createSnapshotCollection(`Snapshot: ${label}`, docIds);
    if (!collectionId) {
      toast.warning('Snapshot not created', { message: 'That name is already in use.' });
      return;
    }

    toast.success(`${plural(docIds.length, 'document')} captured`);
    clearSelection();
  }, [clearSelection, createSnapshotCollection, searchQuery, selectedDocumentIds]);

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

    setIsAssigningSpace(true);
    const result = await VaultAPI.setDocumentsSpaceMembership(selectedIds, spaceId, true);
    if (result.ok) {
      const spaceName = spaces.find((space) => space.id === spaceId)?.name ?? 'space';
      toast.success(`${plural(selectedIds.length, 'document')} added to ${spaceName}`);
      clearSelection();
    } else {
      toast.error('Failed to add documents to space', { message: result.error });
    }
    setIsAssigningSpace(false);
  }, [clearSelection, selectedDocumentIds, spaces]);

  const executeBulkDelete = useCallback(async () => {
    if (!pendingBulkDelete) return;

    setIsDeleting(true);

    let successCount = 0;
    let errorCount = 0;
    const errors: string[] = [];

    for (const docId of pendingBulkDelete.ids) {
      const result = await VaultAPI.deleteDocument(docId);
      if (result.ok) {
        successCount++;
      } else {
        errorCount++;
        errors.push(result.error || 'Unknown error');
      }
    }

    if (successCount > 0) {
      toast.success(`${plural(successCount, 'document')} deleted`);
      clearSelection();
      await refreshFiles();
    }

    if (errorCount > 0) {
      toast.error(`Failed to delete ${plural(errorCount, 'document')}`, {
        message: errors.slice(0, 3).join(', ') + (errors.length > 3 ? '…' : ''),
      });
    }

    setIsDeleting(false);
    setPendingBulkDelete(null);
  }, [pendingBulkDelete, clearSelection, refreshFiles]);

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

    setIsDeleting(true);
    const result = await VaultAPI.deleteDocument(pendingDeleteDoc.id);
    if (result.ok) {
      toast.success('Document deleted');
      await refreshFiles();
    } else {
      toast.error('Failed to delete document', { message: result.error });
    }
    setIsDeleting(false);
    setPendingDeleteDoc(null);
  }, [pendingDeleteDoc, refreshFiles]);

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

        <div className="flex min-h-0 flex-1 gap-6">
          {isRailOpen ? (
            <LibraryRail
              scope={scope}
              onScopeChange={setScope}
              collections={customCollections}
              onCreateCollection={handleCreateCollection}
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
            <div className="flex items-baseline gap-2">
              <h2 className="pb-3 text-lg font-medium text-text-secondary">{scopeName}</h2>
              <span className="text-sm tabular-nums text-text-muted">
                {filteredDocuments.length.toLocaleString()}
              </span>
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
              {viewMode === 'tree' && <TreeView {...viewProps} />}
              {viewMode === 'list' && <ListView {...viewProps} />}
              {viewMode === 'grid' && (
                <GridView
                  onFileOpen={viewProps.onFileOpen}
                  onContextMenu={viewProps.onContextMenu}
                  onAddFolder={viewProps.onAddFolder}
                />
              )}
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
          onIndexChanged={invalidateLibrary}
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
        message={`Delete ${plural(pendingBulkDelete?.count ?? 0, 'document')}? They and their embeddings are removed from the index.`}
        confirmLabel="Delete"
        cancelLabel="Cancel"
        variant="danger"
        confirmDisabled={isDeleting}
        onConfirm={executeBulkDelete}
        onCancel={() => setPendingBulkDelete(null)}
      />
    </main>
  );
}
