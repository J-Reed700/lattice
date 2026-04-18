/**
 * File Browser Component
 *
 * Purpose: Main file browser interface with multiple view modes
 *
 * Features:
 * - Tree/List/Grid view modes
 * - Breadcrumb navigation
 * - Search and filter
 * - Context menu
 * - Keyboard shortcuts
 * - Responsive layout
 */

import { useState, useCallback, useEffect, useMemo, useRef } from 'react';

import { open as openExternal } from '@tauri-apps/plugin-shell';
import { FileUp } from 'lucide-react';
import { useNavigate } from 'react-router-dom';

import { ContextMenu } from './ContextMenu';
import { GridView } from './GridView';
import { LibraryToolbar } from './LibraryToolbar';
import { ListView } from './ListView';
import { RenameDialog } from './RenameDialog';
import { SavedSearchNameDialog, type SavedSearchNameDialogMode } from './SavedSearchNameDialog';
import { TreeView } from './TreeView';
import VaultAPI from '../../lib/api';
import { useFileBrowserStore, selectFilteredDocuments } from '../../stores/fileBrowserStore';
import { toast } from '../../stores/toastStore';
import { type ConversationSpaceDto } from '../../types';
import { type DocumentMetadata } from '../../types/fileBrowser';
import { isSupportedFileType } from '../../utils/fileTypeDetector';
import { handleAsyncEvent } from '../../utils/promiseHandlers';
import { ConfirmDialog } from '../ConfirmDialog';
import { ContentViewer } from '../ContentViewer';
import Button from '../ui/Button/Button';

const isAbsolutePath = (path: string): boolean =>
  path.startsWith('/') || /^[A-Za-z]:[\\/]/.test(path);

const isHttpUrl = (path: string): boolean => /^https?:\/\//i.test(path);

const normalizeCategory = (category: string): string =>
  category.toLowerCase().replace(/[_-]+/g, ' ').trim();

const normalizeSearchToken = (value: string): string => value.trim().toLowerCase();

const NON_TEXT_FILE_TYPES = new Set([
  'png',
  'jpg',
  'jpeg',
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
  'zip',
  'rar',
  '7z',
  'tar',
  'gz',
]);

const isWebDocument = (doc: DocumentMetadata): boolean => {
  const normalizedCategory = normalizeCategory(doc.category || '');
  return (
    normalizedCategory.includes('web article') ||
    normalizedCategory === 'web' ||
    isHttpUrl(doc.filePath) ||
    doc.filePath.includes('/.recall/web-archive/')
  );
};

const matchesTypeFilter = (doc: DocumentMetadata, activeFilter: string | null): boolean => {
  if (!activeFilter) {
    return true;
  }

  const normalizedCategory = normalizeCategory(doc.category);
  const normalizedFileType = (doc.fileType ?? '').toLowerCase();

  return (
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

export function FileBrowser() {
  const documents = useFileBrowserStore(state => state.documents);
  const filteredDocuments = useFileBrowserStore(selectFilteredDocuments);
  const viewMode = useFileBrowserStore(state => state.viewMode);
  const setViewMode = useFileBrowserStore(state => state.setViewMode);
  const density = useFileBrowserStore(state => state.density);
  const setDensity = useFileBrowserStore(state => state.setDensity);
  const selectedDocumentIds = useFileBrowserStore(state => state.selectedDocumentIds);
  const clearSelection = useFileBrowserStore(state => state.clearSelection);
  const createSnapshotCollection = useFileBrowserStore(state => state.createSnapshotCollection);
  const searchQuery = useFileBrowserStore(state => state.searchQuery);
  const setSearchQuery = useFileBrowserStore(state => state.setSearchQuery);
  const filterByType = useFileBrowserStore(state => state.filterByType);
  const filterBySource = useFileBrowserStore(state => state.filterBySource);
  const setFilterBySource = useFileBrowserStore(state => state.setFilterBySource);
  const groupByDate = useFileBrowserStore(state => state.groupByDate);
  const toggleGroupByDate = useFileBrowserStore(state => state.toggleGroupByDate);
  const savedViews = useFileBrowserStore(state => state.savedViews);
  const activeSavedViewId = useFileBrowserStore(state => state.activeSavedViewId);
  const applySavedView = useFileBrowserStore(state => state.applySavedView);
  const clearActiveSavedView = useFileBrowserStore(state => state.clearActiveSavedView);
  const savedSearches = useFileBrowserStore(state => state.savedSearches);
  const activeSavedSearchId = useFileBrowserStore(state => state.activeSavedSearchId);
  const createSavedSearch = useFileBrowserStore(state => state.createSavedSearch);
  const applySavedSearch = useFileBrowserStore(state => state.applySavedSearch);
  const updateSavedSearch = useFileBrowserStore(state => state.updateSavedSearch);
  const duplicateSavedSearch = useFileBrowserStore(state => state.duplicateSavedSearch);
  const deleteSavedSearch = useFileBrowserStore(state => state.deleteSavedSearch);
  const reorderSavedSearches = useFileBrowserStore(state => state.reorderSavedSearches);
  const clearActiveSavedSearch = useFileBrowserStore(state => state.clearActiveSavedSearch);
  const isContentSearchLoading = useFileBrowserStore(state => state.isContentSearchLoading);
  const setContentSearchMatches = useFileBrowserStore(state => state.setContentSearchMatches);
  const setContentSearchLoading = useFileBrowserStore(state => state.setContentSearchLoading);
  const clearContentSearch = useFileBrowserStore(state => state.clearContentSearch);
  const contextMenuPosition = useFileBrowserStore(state => state.contextMenuPosition);
  const contextMenuDocument = useFileBrowserStore(state => state.contextMenuDocument);
  const openContextMenu = useFileBrowserStore(state => state.openContextMenu);
  const closeContextMenu = useFileBrowserStore(state => state.closeContextMenu);
  const refreshFiles = useFileBrowserStore(state => state.refreshFiles);
  const loadFiles = useFileBrowserStore(state => state.loadFiles);

  const [viewerFilePath, setViewerFilePath] = useState<string | null>(null);
  const [renameDocument, setRenameDocument] = useState<DocumentMetadata | null>(null);
  const [pendingDeleteDoc, setPendingDeleteDoc] = useState<DocumentMetadata | null>(null);
  const [pendingBulkDelete, setPendingBulkDelete] = useState<{ ids: string[]; count: number } | null>(null);
  const [pendingDeleteSavedSearch, setPendingDeleteSavedSearch] = useState<{ id: string; name: string } | null>(null);
  const [savedSearchNameDialogMode, setSavedSearchNameDialogMode] = useState<SavedSearchNameDialogMode | null>(null);
  const [savedSearchNameDialogValue, setSavedSearchNameDialogValue] = useState('');
  const [savedSearchNameDialogTargetId, setSavedSearchNameDialogTargetId] = useState<string | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);
  const [spaces, setSpaces] = useState<ConversationSpaceDto[]>([]);
  const [selectedBulkSpaceId, setSelectedBulkSpaceId] = useState('');
  const [isLoadingSpaces, setIsLoadingSpaces] = useState(false);
  const [isAssigningSpace, setIsAssigningSpace] = useState(false);
  const [draggedSearchId, setDraggedSearchId] = useState<string | null>(null);

  const contentCacheRef = useRef<Map<string, string>>(new Map());
  const contentReadFailuresRef = useRef<Set<string>>(new Set());
  const contentSearchSequenceRef = useRef(0);
  const documentSpaceMembershipCacheRef = useRef<Map<string, string[]>>(new Map());
  const bulkSpaceSelectionSequenceRef = useRef(0);

  const navigate = useNavigate();

  // Load files on initial mount only
  useEffect(() => {
    if (!activeSavedViewId && viewMode !== 'tree') {
      setViewMode('tree');
    }
    void loadFiles();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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

    for (const id of documentSpaceMembershipCacheRef.current.keys()) {
      if (!currentIds.has(id)) {
        documentSpaceMembershipCacheRef.current.delete(id);
      }
    }
  }, [documents]);

  useEffect(() => {
    const selectedIds = Array.from(selectedDocumentIds);
    const requestId = bulkSpaceSelectionSequenceRef.current + 1;
    bulkSpaceSelectionSequenceRef.current = requestId;
    let cancelled = false;

    const inferSelectedSpace = async () => {
      if (selectedIds.length === 0) {
        setSelectedBulkSpaceId('');
        return;
      }

      if (spaces.length === 0) {
        setSelectedBulkSpaceId('');
        return;
      }

      const validSpaceIds = new Set(spaces.map((space) => space.id));

      const membershipsByDocument = await Promise.all(
        selectedIds.map(async (documentId) => {
          const cached = documentSpaceMembershipCacheRef.current.get(documentId);
          if (cached) {
            return cached;
          }

          const result = await VaultAPI.listDocumentSpaceMemberships(documentId);
          if (!result.ok) {
            return [];
          }

          const spaceIds = result.data.map((membership) => membership.spaceId);
          documentSpaceMembershipCacheRef.current.set(documentId, spaceIds);
          return spaceIds;
        })
      );

      if (cancelled || bulkSpaceSelectionSequenceRef.current !== requestId) {
        return;
      }

      const normalizedMemberships = membershipsByDocument.map((spaceIds) =>
        spaceIds.filter((spaceId) => validSpaceIds.has(spaceId))
      );

      if (normalizedMemberships.length === 0) {
        setSelectedBulkSpaceId('');
        return;
      }

      let sharedSpaceIds = new Set(normalizedMemberships[0]);
      for (let index = 1; index < normalizedMemberships.length; index += 1) {
        const next = new Set(normalizedMemberships[index]);
        sharedSpaceIds = new Set([...sharedSpaceIds].filter((spaceId) => next.has(spaceId)));
      }

      const inferredSpaceId = spaces.find((space) => sharedSpaceIds.has(space.id))?.id ?? '';
      setSelectedBulkSpaceId(inferredSpaceId);
    };

    void inferSelectedSpace();

    return () => {
      cancelled = true;
    };
  }, [selectedDocumentIds, spaces]);

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

  const filteredCount = filteredDocuments.length;
  const pinnedSearches = useMemo(
    () => savedSearches.filter((search) => search.pinned),
    [savedSearches]
  );
  const activeSavedSearch = useMemo(
    () => savedSearches.find((search) => search.id === activeSavedSearchId) ?? null,
    [activeSavedSearchId, savedSearches]
  );
  const activeSavedViewName = useMemo(
    () => savedViews.find((view) => view.id === activeSavedViewId)?.name ?? null,
    [activeSavedViewId, savedViews]
  );

  const handleFileOpen = useCallback(async (doc: DocumentMetadata) => {
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
  }, []);

  const handleContextMenu = useCallback(
    (event: React.MouseEvent, doc: DocumentMetadata) => {
      event.preventDefault();
      const fileNode = {
        id: doc.id,
        name: doc.fileName,
        path: doc.filePath,
        type: 'file' as const,
        size: 0,
        modified: doc.modifiedAt,
        extension: doc.fileType,
        isIndexed: true,
      };
      openContextMenu({ x: event.clientX, y: event.clientY }, fileNode);
    },
    [openContextMenu]
  );

  const handleRefresh = useCallback(async () => {
    await refreshFiles();
  }, [refreshFiles]);

  const handleSaveResultsAsCollection = useCallback(() => {
    const docIds = filteredDocuments.map((doc) => doc.id);
    if (docIds.length === 0) {
      toast.warning('No results to freeze');
      return;
    }

    const queryLabel = searchQuery.trim();
    const timestamp = new Date().toLocaleString();
    const suggestedName = queryLabel
      ? `Frozen: ${queryLabel}`
      : `Frozen ${timestamp}`;

    const collectionId = createSnapshotCollection(suggestedName, docIds);
    if (!collectionId) {
      toast.warning('Frozen collection not created', {
        message: 'Name may already exist at this level.',
      });
      return;
    }

    toast.success('Frozen collection created', {
      message: `${docIds.length} documents captured as a locked collection.`,
    });
  }, [createSnapshotCollection, filteredDocuments, searchQuery]);

  const handleSavedViewChange = useCallback((value: string) => {
    if (!value) {
      clearActiveSavedView();
      return;
    }
    applySavedView(value);
  }, [applySavedView, clearActiveSavedView]);

  const handleSavedSearchChange = useCallback((value: string) => {
    if (!value) {
      clearActiveSavedSearch();
      return;
    }
    applySavedSearch(value);
  }, [applySavedSearch, clearActiveSavedSearch]);

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
          message: 'Name may already exist, or no query/filter is active.',
        });
        return;
      }
      toast.success('Saved search created');
      closeSavedSearchNameDialog();
      return;
    }

    if (savedSearchNameDialogMode === 'rename' && savedSearchNameDialogTargetId) {
      updateSavedSearch(savedSearchNameDialogTargetId, { name });
      toast.success('Saved search renamed');
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

  const handleSaveSearch = useCallback(() => {
    const hasCriteria = Boolean(searchQuery.trim()) || Boolean(activeFilter) || filterBySource !== 'all';
    if (!hasCriteria) {
      toast.warning('No active search to save');
      return;
    }

    const suggestedName = searchQuery.trim()
      ? `Search: ${searchQuery.trim()}`
      : `Search ${new Date().toLocaleString()}`;
    openSavedSearchNameDialog('create', suggestedName);
  }, [activeFilter, filterBySource, openSavedSearchNameDialog, searchQuery]);

  const handleReorderPinnedSearch = useCallback((sourceId: string, targetId: string) => {
    if (sourceId === targetId) {
      return;
    }

    const pinnedIds = pinnedSearches.map((search) => search.id);
    const sourceIndex = pinnedIds.indexOf(sourceId);
    const targetIndex = pinnedIds.indexOf(targetId);
    if (sourceIndex < 0 || targetIndex < 0) {
      return;
    }

    const nextPinnedIds = [...pinnedIds];
    const [moved] = nextPinnedIds.splice(sourceIndex, 1);
    nextPinnedIds.splice(targetIndex, 0, moved);

    const unpinnedIds = savedSearches
      .filter((search) => !search.pinned)
      .map((search) => search.id);
    reorderSavedSearches([...nextPinnedIds, ...unpinnedIds]);
  }, [pinnedSearches, reorderSavedSearches, savedSearches]);

  const handleMovePinnedSearchByDirection = useCallback((searchId: string, direction: 'left' | 'right') => {
    const pinnedIds = pinnedSearches.map((search) => search.id);
    const sourceIndex = pinnedIds.indexOf(searchId);
    if (sourceIndex < 0) {
      return;
    }

    const targetIndex = direction === 'left' ? sourceIndex - 1 : sourceIndex + 1;
    if (targetIndex < 0 || targetIndex >= pinnedIds.length) {
      return;
    }

    const nextPinnedIds = [...pinnedIds];
    const [moved] = nextPinnedIds.splice(sourceIndex, 1);
    nextPinnedIds.splice(targetIndex, 0, moved);

    const unpinnedIds = savedSearches
      .filter((search) => !search.pinned)
      .map((search) => search.id);
    reorderSavedSearches([...nextPinnedIds, ...unpinnedIds]);
  }, [pinnedSearches, reorderSavedSearches, savedSearches]);

  const handleRenamePinnedSearch = useCallback((searchId: string, name: string) => {
    openSavedSearchNameDialog('rename', name, searchId);
  }, [openSavedSearchNameDialog]);

  const handleRenameActiveSearch = useCallback(() => {
    if (!activeSavedSearch) {
      return;
    }
    openSavedSearchNameDialog('rename', activeSavedSearch.name, activeSavedSearch.id);
  }, [activeSavedSearch, openSavedSearchNameDialog]);

  const handleDuplicateActiveSearch = useCallback(() => {
    if (!activeSavedSearch) {
      return;
    }

    const duplicateId = duplicateSavedSearch(activeSavedSearch.id);
    if (!duplicateId) {
      toast.warning('Saved search duplication failed');
      return;
    }

    toast.success('Saved search duplicated');
  }, [activeSavedSearch, duplicateSavedSearch]);

  const handleTogglePinActiveSearch = useCallback(() => {
    if (!activeSavedSearch) {
      return;
    }

    updateSavedSearch(activeSavedSearch.id, { pinned: !activeSavedSearch.pinned });
  }, [activeSavedSearch, updateSavedSearch]);

  const handleDeleteActiveSearch = useCallback(() => {
    if (!activeSavedSearch) {
      return;
    }
    setPendingDeleteSavedSearch({
      id: activeSavedSearch.id,
      name: activeSavedSearch.name,
    });
  }, [activeSavedSearch]);

  const executeDeleteSavedSearch = useCallback(() => {
    if (!pendingDeleteSavedSearch) {
      return;
    }

    deleteSavedSearch(pendingDeleteSavedSearch.id);
    toast.success('Saved search deleted');
    setPendingDeleteSavedSearch(null);
  }, [deleteSavedSearch, pendingDeleteSavedSearch]);

  const handleBulkDelete = useCallback(async () => {
    const selectedIds = Array.from(selectedDocumentIds);
    const count = selectedIds.length;

    if (count === 0) return;

    setPendingBulkDelete({ ids: selectedIds, count });
  }, [selectedDocumentIds]);

  const handleBulkAssignToSpace = useCallback(async () => {
    const selectedIds = Array.from(selectedDocumentIds);
    if (selectedIds.length === 0) {
      toast.warning('No documents selected');
      return;
    }

    if (!selectedBulkSpaceId) {
      toast.warning('Choose a space first');
      return;
    }

    setIsAssigningSpace(true);
    const result = await VaultAPI.setDocumentsSpaceMembership(selectedIds, selectedBulkSpaceId, true);
    if (result.ok) {
      for (const documentId of selectedIds) {
        const existingSpaceIds = documentSpaceMembershipCacheRef.current.get(documentId) ?? [];
        if (!existingSpaceIds.includes(selectedBulkSpaceId)) {
          documentSpaceMembershipCacheRef.current.set(documentId, [...existingSpaceIds, selectedBulkSpaceId]);
        }
      }
      const spaceName = spaces.find((space) => space.id === selectedBulkSpaceId)?.name ?? 'selected space';
      toast.success(
        `${selectedIds.length} document${selectedIds.length === 1 ? '' : 's'} assigned to ${spaceName}`
      );
      clearSelection();
    } else {
      toast.error('Failed to assign selected documents', { message: result.error });
    }
    setIsAssigningSpace(false);
  }, [clearSelection, selectedBulkSpaceId, selectedDocumentIds, spaces]);

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
      toast.success(`${successCount} document${successCount === 1 ? '' : 's'} deleted successfully`);
      clearSelection();
      await refreshFiles();
    }

    if (errorCount > 0) {
      toast.error(`Failed to delete ${errorCount} document${errorCount === 1 ? '' : 's'}`, {
        message: errors.slice(0, 3).join(', ') + (errors.length > 3 ? '...' : ''),
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
      toast.success('Document deleted successfully');
      await refreshFiles();
    } else {
      toast.error('Failed to delete document', { message: result.error });
    }
    setIsDeleting(false);
    setPendingDeleteDoc(null);
  }, [pendingDeleteDoc, refreshFiles]);

  return (
    <div className="flex h-full flex-col bg-[hsl(var(--surface))]">
      <LibraryToolbar
        filteredCount={filteredCount}
        totalCount={documents.length}
        selectedCount={selectedDocumentIds.size}
        savedViews={savedViews}
        activeSavedViewId={activeSavedViewId}
        activeSavedViewName={activeSavedViewName}
        onSavedViewChange={handleSavedViewChange}
        onClearActiveSavedView={clearActiveSavedView}
        savedSearches={savedSearches}
        activeSavedSearchId={activeSavedSearchId}
        activeSavedSearchName={activeSavedSearch?.name ?? null}
        onSavedSearchChange={handleSavedSearchChange}
        onClearActiveSavedSearch={clearActiveSavedSearch}
        onRenameActiveSearch={handleRenameActiveSearch}
        onDuplicateActiveSearch={handleDuplicateActiveSearch}
        onTogglePinActiveSearch={handleTogglePinActiveSearch}
        onDeleteActiveSearch={handleDeleteActiveSearch}
        onSaveSearch={handleSaveSearch}
        saveSearchDisabled={!searchQuery.trim() && !activeFilter && filterBySource === 'all'}
        onSaveResultsAsCollection={handleSaveResultsAsCollection}
        saveResultsDisabled={filteredCount === 0}
        searchQuery={searchQuery}
        onSearchQueryChange={setSearchQuery}
        onClearSearch={() => setSearchQuery('')}
        isContentSearchLoading={isContentSearchLoading}
        hasNormalizedSearchQuery={Boolean(normalizedSearchQuery)}
        sourceCounts={sourceCounts}
        filterBySource={filterBySource}
        onFilterBySourceChange={setFilterBySource}
        groupByDate={groupByDate}
        onToggleGroupByDate={toggleGroupByDate}
        viewMode={viewMode}
        onViewModeChange={setViewMode}
        density={density}
        onDensityChange={setDensity}
        onRefresh={handleAsyncEvent(handleRefresh)}
        normalizedSearchQuery={normalizedSearchQuery}
        activeFilter={activeFilter}
        pinnedSearches={pinnedSearches}
        draggedSearchId={draggedSearchId}
        onSetDraggedSearchId={setDraggedSearchId}
        onReorderPinnedSearch={handleReorderPinnedSearch}
        onMovePinnedSearchByDirection={handleMovePinnedSearchByDirection}
        onRenamePinnedSearch={handleRenamePinnedSearch}
        onApplySavedSearch={applySavedSearch}
      />

      {selectedDocumentIds.size > 0 && (
        <div className="mx-4 mt-2 flex flex-wrap items-center gap-2 rounded-xl border border-[hsl(var(--accent))]/20 bg-[hsl(var(--accent-muted))]/25 px-3 py-2">
          <span className="text-sm font-semibold text-[hsl(var(--text-primary))]">
            {selectedDocumentIds.size} selected
          </span>
          <select
            value={selectedBulkSpaceId}
            onChange={(event) => setSelectedBulkSpaceId(event.target.value)}
            disabled={isLoadingSpaces || isAssigningSpace || spaces.length === 0}
            className="h-9 min-w-[180px] rounded-lg border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-2 text-sm text-[hsl(var(--text-primary))]"
            aria-label="Bulk assign space"
          >
            <option value="">
              {isLoadingSpaces ? 'Loading spaces...' : 'Choose space'}
            </option>
            {spaces.map((space) => (
              <option key={space.id} value={space.id}>
                {space.name}{space.isArchived ? ' (Archived)' : ''}
              </option>
            ))}
          </select>
          <Button
            variant="ghost"
            size="sm"
            onClick={handleAsyncEvent(handleBulkAssignToSpace)}
            disabled={isAssigningSpace || !selectedBulkSpaceId || isLoadingSpaces || spaces.length === 0}
            className="h-9 rounded-lg border border-[hsl(var(--accent))]/30 px-3 text-sm text-[hsl(var(--accent))] hover:bg-[hsl(var(--accent-muted))]/40"
          >
            Assign to Space
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={handleAsyncEvent(handleBulkDelete)}
            className="h-9 rounded-lg border border-[hsl(var(--danger-fg))]/30 px-3 text-sm text-[hsl(var(--danger-fg))] hover:bg-[hsl(var(--danger-muted))]"
          >
            Delete Selected
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={clearSelection}
            className="h-9 rounded-lg border border-[hsl(var(--border-subtle))] px-3 text-sm text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))]"
          >
            Clear Selection
          </Button>
        </div>
      )}

      <div className="flex-1 overflow-hidden px-4 pb-4 pt-0.5">
        <div className="h-full overflow-hidden rounded-2xl border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] shadow-[var(--shadow-sm)]">
          {documents.length === 0 ? (
            <div className="flex h-full items-center justify-center">
              <div className="max-w-md p-8 text-center">
                <div className="mb-4 flex justify-center">
                  <div className="rounded-full border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))] p-4">
                    <FileUp className="h-12 w-12 text-[hsl(var(--text-tertiary))]" />
                  </div>
                </div>
                <h3 className="mb-2 text-xl font-semibold">No Documents Yet</h3>
                <p className="mb-6 text-[hsl(var(--text-secondary))]">
                  Get started by adding files to your knowledge base.
                </p>
                <Button onClick={() => navigate('/ingest')} className="inline-flex items-center gap-2">
                  <FileUp className="h-4 w-4" />
                  Add Files
                </Button>
              </div>
            </div>
          ) : (
            <>
              {viewMode === 'tree' && (
                <TreeView
                  onFileOpen={handleAsyncEvent(handleFileOpen)}
                  onContextMenu={handleContextMenu}
                  onRename={handleRename}
                  onDelete={handleAsyncEvent(handleDelete)}
                />
              )}
              {viewMode === 'list' && (
                <ListView
                  onFileOpen={handleAsyncEvent(handleFileOpen)}
                  onContextMenu={handleContextMenu}
                  onRename={handleRename}
                  onDelete={handleAsyncEvent(handleDelete)}
                />
              )}
              {viewMode === 'grid' && (
                <GridView onFileOpen={handleAsyncEvent(handleFileOpen)} onContextMenu={handleContextMenu} />
              )}
            </>
          )}
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
        message={`Delete "${pendingDeleteDoc?.fileName ?? 'this document'}"? This will permanently remove the document and its embeddings from the search index.`}
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
        message={`Delete ${pendingBulkDelete?.count ?? 0} document${(pendingBulkDelete?.count ?? 0) === 1 ? '' : 's'}? This will permanently remove the selected documents and their embeddings from the search index.`}
        confirmLabel="Delete"
        cancelLabel="Cancel"
        variant="danger"
        confirmDisabled={isDeleting}
        onConfirm={executeBulkDelete}
        onCancel={() => setPendingBulkDelete(null)}
      />

      <ConfirmDialog
        isOpen={pendingDeleteSavedSearch !== null}
        title="Delete saved search?"
        message={`Delete "${pendingDeleteSavedSearch?.name ?? 'this saved search'}"? This preset will be removed from your library toolbar and views.`}
        confirmLabel="Delete"
        cancelLabel="Cancel"
        variant="danger"
        onConfirm={executeDeleteSavedSearch}
        onCancel={() => setPendingDeleteSavedSearch(null)}
      />
    </div>
  );
}
